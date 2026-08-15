use smol_str::{SmolStr, ToSmolStr};
use swc_common::{Span, Spanned};
use swc_ecma_ast::{AssignExpr, Expr, Lit, ObjectLit, Prop, PropOrSpread};

use crate::analysis::{
    model::module::{COMMONJS_EXPORTS, COMMONJS_MODULE, ModuleExport, ModuleInterface},
    resolution::Resolver,
    syntax::{literal_member_property_name, literal_property_name},
};

impl ModuleInterface {
    pub(in crate::analysis::facts) fn record_commonjs_export(
        &mut self,
        assignment: &AssignExpr,
        resolver: &Resolver,
    ) {
        use swc_ecma_ast::AssignOp;

        if assignment.op != AssignOp::Assign {
            return;
        }
        if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(ident)) =
            &assignment.left
            && (resolver.is_unshadowed_commonjs_name(&ident.id, COMMONJS_EXPORTS)
                || resolver.is_unshadowed_commonjs_name(&ident.id, COMMONJS_MODULE))
        {
            self.mark_unknown_exports();
            return;
        }
        let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(member)) =
            &assignment.left
        else {
            return;
        };
        let prop = literal_member_property_name(&member.prop);
        if is_commonjs_name(&member.obj, COMMONJS_MODULE, resolver)
            && prop.as_deref() == Some(COMMONJS_EXPORTS)
        {
            self.record_module_exports_assignment(assignment, resolver);
            return;
        }
        if is_commonjs_name(&member.obj, COMMONJS_EXPORTS, resolver) {
            self.record_commonjs_property_export(assignment, prop, resolver);
            return;
        }
        let Expr::Member(parent) = &*member.obj else {
            return;
        };
        if !is_commonjs_name(&parent.obj, COMMONJS_MODULE, resolver)
            || literal_member_property_name(&parent.prop).as_deref() != Some(COMMONJS_EXPORTS)
        {
            return;
        }
        let Some(property) = prop else {
            self.mark_unknown_exports();
            return;
        };
        self.record_commonjs_property_export(assignment, Some(property), resolver);
    }

    fn record_module_exports_assignment(&mut self, assignment: &AssignExpr, resolver: &Resolver) {
        if self.has_exports() {
            self.mark_unknown_exports();
            return;
        }
        if let Expr::Object(object) = &*assignment.right {
            let Some(entries) = Self::collect_commonjs_export_entries(object) else {
                self.mark_unknown_exports();
                return;
            };
            self.add_export("default", ModuleExport::Value);
            for entry in entries {
                if let Some(span) = entry.value_span {
                    add_function_export_if_span(self, &entry.name, span, resolver);
                }
                if let Some(ref local) = entry.local {
                    add_function_export_if_name(
                        self,
                        &entry.name,
                        local,
                        assignment.span(),
                        resolver,
                    );
                }
                if let Some(value) = entry.static_value {
                    self.add_static_string(entry.name.clone(), value);
                }
                self.add_export(
                    entry.name,
                    entry
                        .local
                        .map_or(ModuleExport::Value, |n| ModuleExport::Local { name: n }),
                );
            }
        } else {
            if let Some(id) = resolver.function_id_for_span(assignment.right.span()) {
                self.add_function_export("default", id);
            }
            self.add_export("default", ModuleExport::Value);
        }
    }

    fn record_commonjs_property_export(
        &mut self,
        assignment: &AssignExpr,
        property: Option<SmolStr>,
        resolver: &Resolver,
    ) {
        let Some(property) = property else {
            self.mark_unknown_exports();
            return;
        };
        let export = match &*assignment.right {
            Expr::Ident(ident) => {
                add_function_export_if_name(
                    self,
                    &property,
                    ident.sym.as_ref(),
                    assignment.span(),
                    resolver,
                );
                ModuleExport::Local {
                    name: ident.sym.to_smolstr(),
                }
            }
            expr => {
                add_function_export_if_expr(self, &property, expr, resolver);
                if let Expr::Lit(Lit::Str(value)) = expr {
                    self.add_static_string(property.clone(), value.value.to_string_lossy());
                }
                ModuleExport::Value
            }
        };
        self.add_export(property, export);
    }

    fn collect_commonjs_export_entries(object: &ObjectLit) -> Option<Vec<CommonJsExportEntry>> {
        // Single iterator pipeline mapping each property to an export entry.
        // Extraction would not reduce complexity because the shared match
        // context (literal_property_name calls, CommonJsExportEntry construction)
        // would need to be repeated in every helper.
        object
            .props
            .iter()
            .map(|prop| match prop {
                PropOrSpread::Prop(prop) => match &**prop {
                    Prop::KeyValue(value) => {
                        let name = literal_property_name(&value.key)?;
                        let (local, static_value) = match &*value.value {
                            Expr::Ident(ident) => (Some(ident.sym.to_smolstr()), None),
                            Expr::Lit(Lit::Str(s)) => {
                                (None, Some(s.value.to_string_lossy().into_owned()))
                            }
                            _ => (None, None),
                        };
                        Some(CommonJsExportEntry {
                            name,
                            local,
                            value_span: Some(value.value.span()),
                            static_value,
                        })
                    }
                    Prop::Assign(assign) => Some(CommonJsExportEntry {
                        name: assign.key.sym.to_smolstr(),
                        local: Some(assign.key.sym.to_smolstr()),
                        value_span: None,
                        static_value: None,
                    }),
                    Prop::Getter(getter) => Some(CommonJsExportEntry {
                        name: literal_property_name(&getter.key)?,
                        local: None,
                        value_span: None,
                        static_value: None,
                    }),
                    Prop::Setter(setter) => Some(CommonJsExportEntry {
                        name: literal_property_name(&setter.key)?,
                        local: None,
                        value_span: None,
                        static_value: None,
                    }),
                    Prop::Method(method) => Some(CommonJsExportEntry {
                        name: literal_property_name(&method.key)?,
                        local: None,
                        value_span: Some(method.function.span()),
                        static_value: None,
                    }),
                    Prop::Shorthand(ident) => Some(CommonJsExportEntry {
                        name: ident.sym.to_smolstr(),
                        local: Some(ident.sym.to_smolstr()),
                        value_span: None,
                        static_value: None,
                    }),
                },
                PropOrSpread::Spread(_) => None,
            })
            .collect()
    }
}

struct CommonJsExportEntry {
    name: SmolStr,
    local: Option<SmolStr>,
    value_span: Option<Span>,
    static_value: Option<String>,
}

fn is_commonjs_name(expr: &Expr, name: &str, resolver: &Resolver) -> bool {
    matches!(expr, Expr::Ident(ident) if resolver.is_unshadowed_commonjs_name(ident, name))
}

fn add_function_export_if_name(
    interface: &mut ModuleInterface,
    export: &str,
    local: &str,
    span: Span,
    resolver: &Resolver,
) {
    if let Some(id) = resolver.function_id_for_name(local, span) {
        interface.add_function_export(export, id);
    }
}

fn add_function_export_if_expr(
    interface: &mut ModuleInterface,
    export: &str,
    expr: &Expr,
    resolver: &Resolver,
) {
    add_function_export_if_span(interface, export, expr.span(), resolver);
}

fn add_function_export_if_span(
    interface: &mut ModuleInterface,
    export: &str,
    span: Span,
    resolver: &Resolver,
) {
    if let Some(id) = resolver.function_id_for_span(span) {
        interface.add_function_export(export, id);
    }
}
