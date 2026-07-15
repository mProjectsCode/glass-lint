//! Module-interface recording performed during the canonical fact walk.

use super::FactBuilder;
use swc_common::{Span, Spanned};
use swc_ecma_ast::{CallExpr, Callee, Expr, ImportDecl};

impl FactBuilder<'_> {
    pub(super) fn record_local_imports(&mut self, import: &ImportDecl) {
        for specifier in &import.specifiers {
            if !specifier.is_type_only() {
                self.record_local(specifier.local().sym.to_string());
            }
        }
    }

    pub(super) fn record_export_decl(&mut self, declaration: &swc_ecma_ast::Decl) {
        match declaration {
            swc_ecma_ast::Decl::Class(class) => {
                self.record_local(class.ident.sym.to_string());
                self.interface.add_export(
                    class.ident.sym.to_string(),
                    crate::analysis::module::ModuleExport::Local {
                        name: class.ident.sym.to_string(),
                    },
                );
            }
            swc_ecma_ast::Decl::Fn(function) => {
                self.record_local(function.ident.sym.to_string());
                if let Some(id) = self
                    .resolver
                    .function_id_for_expr(&Expr::Ident(function.ident.clone()))
                {
                    self.interface
                        .add_function_export(function.ident.sym.to_string(), id);
                }
                self.interface.add_export(
                    function.ident.sym.to_string(),
                    crate::analysis::module::ModuleExport::Local {
                        name: function.ident.sym.to_string(),
                    },
                );
            }
            swc_ecma_ast::Decl::Var(variable) => {
                for declarator in &variable.decls {
                    self.interface.add_pattern_locals(&declarator.name);
                    let mut names = std::collections::BTreeSet::new();
                    crate::analysis::syntax::collect_pat_bindings(&declarator.name, &mut names);
                    for name in names {
                        if let swc_ecma_ast::Pat::Ident(binding) = &declarator.name
                            && let Some(id) = self
                                .resolver
                                .function_id_for_expr(&Expr::Ident(binding.id.clone()))
                        {
                            self.interface.add_function_export(name.clone(), id);
                        }
                        self.interface.add_export(
                            name.clone(),
                            crate::analysis::module::ModuleExport::Local { name },
                        );
                        if let swc_ecma_ast::Pat::Ident(binding) = &declarator.name
                            && let Some(value) = self
                                .resolver
                                .static_string_value(self.resolver.resolve_ident(&binding.id).id)
                        {
                            self.interface
                                .add_static_string(binding.id.sym.to_string(), value);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn record_module_call_request(&mut self, call: &CallExpr) {
        match &call.callee {
            Callee::Import(_) => {
                let Some(Expr::Lit(swc_ecma_ast::Lit::Str(specifier))) =
                    call.args.first().map(|argument| &*argument.expr)
                else {
                    return;
                };
                self.interface.add_request(
                    specifier.span,
                    crate::project::ResolutionRequestKind::DynamicImport,
                    specifier.value.to_string_lossy(),
                    crate::analysis::module::ModuleRequestRole::DynamicImport,
                );
            }
            Callee::Expr(callee) => {
                let Expr::Ident(ident) = &**callee else {
                    return;
                };
                if !self.resolver.is_unshadowed_commonjs_name(ident, "require") {
                    return;
                }
                if call.args.len() != 1 {
                    return;
                }
                let Some(Expr::Lit(swc_ecma_ast::Lit::Str(specifier))) =
                    call.args.first().map(|argument| &*argument.expr)
                else {
                    return;
                };
                self.interface.add_request(
                    specifier.span,
                    crate::project::ResolutionRequestKind::Require,
                    specifier.value.to_string_lossy(),
                    crate::analysis::module::ModuleRequestRole::Require,
                );
            }
            Callee::Super(_) => {}
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn record_commonjs_export(&mut self, assignment: &swc_ecma_ast::AssignExpr) {
        if assignment.op != swc_ecma_ast::AssignOp::Assign {
            return;
        }
        let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(member)) =
            &assignment.left
        else {
            return;
        };
        let is_unshadowed = |expr: &swc_ecma_ast::Expr, name: &str| matches!(expr, Expr::Ident(ident) if self.resolver.is_unshadowed_commonjs_name(ident, name));
        let property = crate::analysis::syntax::member_prop_name(&member.prop);
        if is_unshadowed(&member.obj, "module") && property.as_deref() == Some("exports") {
            if self.interface.has_exports() {
                self.interface.mark_unknown_exports();
                return;
            }
            if let swc_ecma_ast::Expr::Object(object) = &*assignment.right {
                let Some(entries) = Self::commonjs_object_export_entries(object) else {
                    self.interface.mark_unknown_exports();
                    return;
                };
                self.interface
                    .add_export("default", crate::analysis::module::ModuleExport::Value);
                for prop in &object.props {
                    let swc_ecma_ast::PropOrSpread::Prop(prop) = prop else {
                        continue;
                    };
                    match &**prop {
                        swc_ecma_ast::Prop::KeyValue(value) => {
                            let Some(name) = crate::analysis::syntax::prop_name(&value.key) else {
                                continue;
                            };
                            self.add_function_export_if_expr(&name, &value.value);
                            if let Expr::Lit(swc_ecma_ast::Lit::Str(value)) = &*value.value {
                                self.interface
                                    .add_static_string(name, value.value.to_string_lossy());
                            }
                        }
                        swc_ecma_ast::Prop::Method(method) => {
                            if let Some(name) = crate::analysis::syntax::prop_name(&method.key) {
                                self.add_function_export_if_span(&name, method.function.span());
                            }
                        }
                        _ => {}
                    }
                }
                for (name, local) in entries {
                    if let Some(local) = &local {
                        self.add_function_export_if_name(&name, local, assignment.span());
                    }
                    self.interface.add_export(
                        name,
                        local.map_or(crate::analysis::module::ModuleExport::Value, |name| {
                            crate::analysis::module::ModuleExport::Local { name }
                        }),
                    );
                }
            } else {
                if let Some(id) = self.resolver.function_id_for_span(assignment.right.span()) {
                    self.interface.add_function_export("default", id);
                }
                self.interface
                    .add_export("default", crate::analysis::module::ModuleExport::Value);
            }
            return;
        }
        if is_unshadowed(&member.obj, "exports") {
            if let Some(property) = property {
                let export = match &*assignment.right {
                    Expr::Ident(ident) => {
                        self.add_function_export_if_name(
                            &property,
                            ident.sym.as_ref(),
                            assignment.span(),
                        );
                        crate::analysis::module::ModuleExport::Local {
                            name: ident.sym.to_string(),
                        }
                    }
                    expr => {
                        self.add_function_export_if_expr(&property, expr);
                        if let Expr::Lit(swc_ecma_ast::Lit::Str(value)) = expr {
                            self.interface
                                .add_static_string(&property, value.value.to_string_lossy());
                        }
                        crate::analysis::module::ModuleExport::Value
                    }
                };
                self.interface.add_export(property, export);
            } else {
                self.interface.mark_unknown_exports();
            }
            return;
        }
        let Expr::Member(parent) = &*member.obj else {
            return;
        };
        if !is_unshadowed(&parent.obj, "module")
            || crate::analysis::syntax::member_prop_name(&parent.prop).as_deref() != Some("exports")
        {
            return;
        }
        let Some(property) = property else {
            self.interface.mark_unknown_exports();
            return;
        };
        let export = match &*assignment.right {
            Expr::Ident(ident) => {
                self.add_function_export_if_name(&property, ident.sym.as_ref(), assignment.span());
                crate::analysis::module::ModuleExport::Local {
                    name: ident.sym.to_string(),
                }
            }
            expr => {
                self.add_function_export_if_expr(&property, expr);
                if let Expr::Lit(swc_ecma_ast::Lit::Str(value)) = expr {
                    self.interface
                        .add_static_string(&property, value.value.to_string_lossy());
                }
                crate::analysis::module::ModuleExport::Value
            }
        };
        self.interface.add_export(property, export);
    }

    fn add_function_export_if_name(&mut self, export: &str, local: &str, span: Span) {
        if let Some(id) = self.resolver.function_id_for_name(local, span) {
            self.interface.add_function_export(export, id);
        }
    }

    fn add_function_export_if_expr(&mut self, export: &str, expr: &Expr) {
        self.add_function_export_if_span(export, expr.span());
    }

    fn add_function_export_if_span(&mut self, export: &str, span: Span) {
        if let Some(id) = self.resolver.function_id_for_span(span) {
            self.interface.add_function_export(export, id);
        }
    }

    fn commonjs_object_export_entries(
        object: &swc_ecma_ast::ObjectLit,
    ) -> Option<Vec<(String, Option<String>)>> {
        object
            .props
            .iter()
            .map(|prop| match prop {
                swc_ecma_ast::PropOrSpread::Prop(prop) => match &**prop {
                    swc_ecma_ast::Prop::KeyValue(value) => Some((
                        crate::analysis::syntax::prop_name(&value.key)?,
                        match &*value.value {
                            Expr::Ident(ident) => Some(ident.sym.to_string()),
                            _ => None,
                        },
                    )),
                    swc_ecma_ast::Prop::Assign(assign) => {
                        Some((assign.key.sym.to_string(), Some(assign.key.sym.to_string())))
                    }
                    swc_ecma_ast::Prop::Getter(getter) => {
                        Some((crate::analysis::syntax::prop_name(&getter.key)?, None))
                    }
                    swc_ecma_ast::Prop::Setter(setter) => {
                        Some((crate::analysis::syntax::prop_name(&setter.key)?, None))
                    }
                    swc_ecma_ast::Prop::Method(method) => {
                        Some((crate::analysis::syntax::prop_name(&method.key)?, None))
                    }
                    swc_ecma_ast::Prop::Shorthand(ident) => {
                        Some((ident.sym.to_string(), Some(ident.sym.to_string())))
                    }
                },
                swc_ecma_ast::PropOrSpread::Spread(_) => None,
            })
            .collect()
    }
}
