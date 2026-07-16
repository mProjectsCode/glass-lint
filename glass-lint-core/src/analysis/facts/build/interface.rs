//! Module-interface recording performed during the canonical fact walk.
//!
//! The interface is matcher-independent: it records authored requests and
//! exports for later project linking. Only static, structurally supported
//! shapes are linked; dynamic or conflicting shapes are marked unknown so
//! cross-file analysis fails closed.

use swc_common::{Span, Spanned};
use swc_ecma_ast::{
    CallExpr, Callee, DefaultDecl, ExportAll, ExportDefaultDecl, ExportDefaultExpr,
    ExportSpecifier, Expr, ImportDecl, NamedExport,
};
use swc_ecma_visit::VisitWith;

use super::FactBuilder;
use crate::{
    analysis::module::{ModuleExport, ModuleRequestRole, ReExportBinding},
    project::ResolutionRequestKind,
};

impl FactBuilder<'_> {
    /// Record runtime import bindings as local names for interface linking.
    pub(super) fn record_local_imports(&mut self, import: &ImportDecl) {
        for specifier in &import.specifiers {
            if !specifier.is_type_only() {
                self.record_local(specifier.local().sym.to_string());
            }
        }
    }

    /// Record the export identities exposed by a declaration.
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

    /// Add a resolution request for a literal dynamic import or CommonJS
    /// `require` call; dynamic specifiers remain intentionally unlinked.
    pub(super) fn record_module_call_request(&mut self, call: &CallExpr) {
        // Only literal specifiers become resolution requests. Dynamic module
        // names cannot be linked safely and therefore remain local unknowns.
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

    /// Record local exports and re-exports while ignoring type-only exports.
    pub(super) fn record_named_export(&mut self, export: &NamedExport) {
        if export.type_only {
            return;
        }
        if let Some(source) = export.src.as_ref() {
            self.record_reexports(export, source);
        } else {
            self.record_local_named_exports(&export.specifiers);
        }
    }

    fn record_local_named_exports(&mut self, specifiers: &[ExportSpecifier]) {
        for specifier in specifiers {
            if let ExportSpecifier::Named(named) = specifier
                && !named.is_type_only
            {
                let original = crate::analysis::syntax::module_export_name(&named.orig);
                let exported = named.exported.as_ref().map_or_else(
                    || original.clone(),
                    crate::analysis::syntax::module_export_name,
                );
                if let swc_ecma_ast::ModuleExportName::Ident(ident) = &named.orig
                    && let Some(id) = self
                        .resolver
                        .function_id_for_expr(&Expr::Ident(ident.clone()))
                {
                    self.interface.add_function_export(exported.clone(), id);
                }
                self.interface
                    .add_export(exported, ModuleExport::Local { name: original });
            }
        }
    }

    fn record_reexports(&mut self, export: &NamedExport, source: &swc_ecma_ast::Str) {
        let specifiers = export
            .specifiers
            .iter()
            .filter(|specifier| {
                !matches!(specifier, ExportSpecifier::Named(named) if named.is_type_only)
            })
            .collect::<Vec<_>>();
        if specifiers.is_empty() {
            return;
        }
        let request = self.interface.add_request(
            source.span,
            ResolutionRequestKind::Import,
            source.value.to_string_lossy(),
            ModuleRequestRole::ReExport {
                bindings: export
                    .specifiers
                    .iter()
                    .filter(|specifier| {
                        !matches!(specifier, ExportSpecifier::Named(named) if named.is_type_only)
                    })
                    .map(|specifier| match specifier {
                        ExportSpecifier::Named(named) => ReExportBinding::new(
                            crate::analysis::syntax::module_export_name(&named.orig),
                            named.exported.as_ref().map_or_else(
                                || crate::analysis::syntax::module_export_name(&named.orig),
                                crate::analysis::syntax::module_export_name,
                            ),
                            false,
                        ),
                        ExportSpecifier::Namespace(namespace) => ReExportBinding::new(
                            "*".into(),
                            crate::analysis::syntax::module_export_name(&namespace.name),
                            true,
                        ),
                        ExportSpecifier::Default(default) => ReExportBinding::new(
                            "default".into(),
                            default.exported.sym.to_string(),
                            false,
                        ),
                    })
                    .collect(),
            },
        );
        for specifier in specifiers {
            match specifier {
                ExportSpecifier::Named(named) => {
                    let original = crate::analysis::syntax::module_export_name(&named.orig);
                    let exported = named.exported.as_ref().map_or_else(
                        || original.clone(),
                        crate::analysis::syntax::module_export_name,
                    );
                    self.interface.add_export(
                        exported,
                        ModuleExport::ReExport {
                            request,
                            imported: original,
                        },
                    );
                }
                ExportSpecifier::Namespace(namespace) => self.interface.add_export(
                    crate::analysis::syntax::module_export_name(&namespace.name),
                    ModuleExport::Namespace { request },
                ),
                ExportSpecifier::Default(default) => self.interface.add_export(
                    default.exported.sym.to_string(),
                    ModuleExport::ReExport {
                        request,
                        imported: "default".into(),
                    },
                ),
            }
        }
    }

    /// Record a star export as a deferred request for the project linker.
    pub(super) fn record_export_all(&mut self, export: &ExportAll) {
        if export.type_only {
            return;
        }
        let request = self.interface.add_request(
            export.src.span,
            ResolutionRequestKind::Import,
            export.src.value.to_string_lossy(),
            ModuleRequestRole::StarExport,
        );
        self.interface.add_star_export(request);
    }

    /// Record the default export's supported function, local, or value shape.
    pub(super) fn record_default_expr(&mut self, export: &ExportDefaultExpr) {
        if let Expr::Ident(ident) = &*export.expr {
            if let Some(id) = self
                .resolver
                .function_id_for_expr(&Expr::Ident(ident.clone()))
            {
                self.interface.add_function_export("default", id);
            }
            self.interface.add_export(
                "default",
                ModuleExport::Local {
                    name: ident.sym.to_string(),
                },
            );
        } else {
            if let Some(id) = self.resolver.function_id_for_span(export.expr.span()) {
                self.interface.add_function_export("default", id);
            }
            self.interface.add_export("default", ModuleExport::Value);
        }
        export.expr.visit_with(self);
    }

    /// Record a default declaration without claiming an anonymous value is a
    /// named local when no stable identity exists.
    pub(super) fn record_default_decl(&mut self, export: &ExportDefaultDecl) {
        match &export.decl {
            DefaultDecl::Fn(function) => {
                if let Some(ident) = &function.ident {
                    self.record_local(ident.sym.to_string());
                    if let Some(id) = self
                        .resolver
                        .function_id_for_expr(&Expr::Ident(ident.clone()))
                    {
                        self.interface.add_function_export("default", id);
                    }
                    self.interface.add_export(
                        "default",
                        ModuleExport::Local {
                            name: ident.sym.to_string(),
                        },
                    );
                } else {
                    if let Some(id) = self.resolver.function_id_for_span(function.function.span()) {
                        self.interface.add_function_export("default", id);
                    }
                    self.interface.add_export("default", ModuleExport::Value);
                }
            }
            DefaultDecl::Class(class) => {
                if let Some(ident) = &class.ident {
                    self.record_local(ident.sym.to_string());
                    self.interface.add_export(
                        "default",
                        ModuleExport::Local {
                            name: ident.sym.to_string(),
                        },
                    );
                } else {
                    self.interface.add_export("default", ModuleExport::Value);
                }
            }
            DefaultDecl::TsInterfaceDecl(_) => {
                self.interface.add_export("default", ModuleExport::Unknown);
            }
        }
        export.decl.visit_with(self);
    }

    /// Translate supported CommonJS assignment shapes into interface entries.
    pub(super) fn record_commonjs_export(&mut self, assignment: &swc_ecma_ast::AssignExpr) {
        if assignment.op != swc_ecma_ast::AssignOp::Assign {
            return;
        }
        let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(member)) =
            &assignment.left
        else {
            return;
        };
        let property = crate::analysis::syntax::member_prop_name(&member.prop);
        if self.is_commonjs_name(&member.obj, "module") && property.as_deref() == Some("exports") {
            self.record_module_exports_assignment(assignment);
            return;
        }
        if self.is_commonjs_name(&member.obj, "exports") {
            self.record_exports_assignment(assignment, property);
            return;
        }
        let Expr::Member(parent) = &*member.obj else {
            return;
        };
        if !self.is_commonjs_name(&parent.obj, "module")
            || crate::analysis::syntax::member_prop_name(&parent.prop).as_deref() != Some("exports")
        {
            return;
        }
        let Some(property) = property else {
            self.interface.mark_unknown_exports();
            return;
        };
        self.record_named_module_export(assignment, property);
    }

    fn is_commonjs_name(&self, expr: &swc_ecma_ast::Expr, name: &str) -> bool {
        matches!(expr, Expr::Ident(ident) if self.resolver.is_unshadowed_commonjs_name(ident, name))
    }

    fn record_module_exports_assignment(&mut self, assignment: &swc_ecma_ast::AssignExpr) {
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
    }

    fn record_exports_assignment(
        &mut self,
        assignment: &swc_ecma_ast::AssignExpr,
        property: Option<String>,
    ) {
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

    fn record_named_module_export(
        &mut self,
        assignment: &swc_ecma_ast::AssignExpr,
        property: String,
    ) {
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
