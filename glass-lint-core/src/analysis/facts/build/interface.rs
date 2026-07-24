//! Focused module-interface builder extracted from the general fact walk.
//!
//! [`ModuleInterfaceBuilder`] is the single owner of module-interface policy:
//! it records authored requests and exports for later project linking using
//! a raw resolver query but no fact-stream, traversal, or call-result state.
//! Only static, structurally supported shapes are linked; dynamic or
//! conflicting shapes are marked unknown so cross-file analysis fails closed.

use glass_lint_datastructures::ByteRange;
use smol_str::{SmolStr, ToSmolStr};
use swc_common::{Span, Spanned};
use swc_ecma_ast::{
    DefaultDecl, ExportAll, ExportDefaultDecl, ExportDefaultExpr, ExportSpecifier, Expr,
    ImportDecl, Lit, NamedExport, ObjectLit, Prop, PropOrSpread,
};

use crate::{
    analysis::{
        module::{
            COMMONJS_EXPORTS, COMMONJS_MODULE, DEFAULT_EXPORT, ModuleExport, ModuleRequestId,
            ModuleRequestRole, NAMESPACE_EXPORT, ReExportBinding,
        },
        resolution::Resolver,
        syntax::{collect_pat_bindings, module_export_name, property_name},
    },
    project::ResolutionRequestKind,
};

/// Focused owner of module-interface policy during the canonical fact walk.
///
/// Keeps the export/request recording separate from fact-stream allocation,
/// traversal state, and call-result tracking so interface logic has one
/// cohesive home.
pub(super) struct ModuleInterfaceBuilder {
    interface: crate::analysis::module::ModuleInterface,
}

impl ModuleInterfaceBuilder {
    pub(super) fn new() -> Self {
        Self {
            interface: crate::analysis::module::ModuleInterface::default(),
        }
    }

    pub(super) fn finish(self) -> crate::analysis::module::ModuleInterface {
        self.interface
    }

    // -- Delegated accessors used by FactBuilder --

    pub(super) fn record_local(&mut self, name: impl Into<SmolStr>) {
        self.interface.add_local(name);
    }

    pub(super) fn record_pattern_locals(&mut self, pattern: &swc_ecma_ast::Pat) {
        let mut names = std::collections::BTreeSet::new();
        collect_pat_bindings(pattern, &mut names);
        for name in names {
            self.interface.add_local(name);
        }
    }

    pub(super) fn add_request(
        &mut self,
        span: ByteRange,
        kind: ResolutionRequestKind,
        specifier: impl Into<SmolStr>,
        role: ModuleRequestRole,
    ) -> ModuleRequestId {
        self.interface.add_request(span, kind, specifier, role)
    }

    pub(super) fn mark_unknown_exports(&mut self) {
        self.interface.mark_unknown_exports();
    }

    // -- Interface policy methods --

    /// Record runtime import bindings as local names for interface linking.
    pub(super) fn record_local_imports(&mut self, import: &ImportDecl) {
        for specifier in &import.specifiers {
            if !specifier.is_type_only() {
                self.record_local(specifier.local().sym.to_string());
            }
        }
    }

    /// Record the export identities exposed by a declaration.
    pub(super) fn record_export_decl(
        &mut self,
        declaration: &swc_ecma_ast::Decl,
        resolver: &mut Resolver,
    ) {
        match declaration {
            swc_ecma_ast::Decl::Class(class) => {
                self.record_local(class.ident.sym.to_string());
                self.interface.add_export(
                    class.ident.sym.to_string(),
                    ModuleExport::Local {
                        name: class.ident.sym.to_smolstr(),
                    },
                );
            }
            swc_ecma_ast::Decl::Fn(function) => {
                self.record_local(function.ident.sym.to_string());
                if let Some(id) =
                    resolver.function_id_for_expr(&Expr::Ident(function.ident.clone()))
                {
                    self.interface
                        .add_function_export(function.ident.sym.to_string(), id);
                }
                self.interface.add_export(
                    function.ident.sym.to_string(),
                    ModuleExport::Local {
                        name: function.ident.sym.to_smolstr(),
                    },
                );
            }
            swc_ecma_ast::Decl::Var(variable) => {
                for declarator in &variable.decls {
                    self.record_pattern_locals(&declarator.name);
                    let mut names = std::collections::BTreeSet::new();
                    collect_pat_bindings(&declarator.name, &mut names);
                    for name in names {
                        if let swc_ecma_ast::Pat::Ident(binding) = &declarator.name
                            && let Some(id) =
                                resolver.function_id_for_expr(&Expr::Ident(binding.id.clone()))
                        {
                            self.interface.add_function_export(name.clone(), id);
                        }
                        self.interface
                            .add_export(name.clone(), ModuleExport::Local { name });
                        if let swc_ecma_ast::Pat::Ident(binding) = &declarator.name {
                            let value_id = resolver.resolve_ident_id(&binding.id);
                            if let Some(value) = resolver.static_string_value(value_id) {
                                self.interface
                                    .add_static_string(binding.id.sym.to_string(), value);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Record a dynamic-import request at a resolved byte range.
    pub(super) fn record_import_request(&mut self, span: ByteRange, specifier: &swc_ecma_ast::Str) {
        self.interface.add_request(
            span,
            ResolutionRequestKind::DynamicImport,
            specifier.value.to_string_lossy(),
            ModuleRequestRole::DynamicImport,
        );
    }

    /// Record a CommonJS require request at a resolved byte range.
    pub(super) fn record_require_request(
        &mut self,
        span: ByteRange,
        specifier: &swc_ecma_ast::Str,
    ) {
        self.interface.add_request(
            span,
            ResolutionRequestKind::Require,
            specifier.value.to_string_lossy(),
            ModuleRequestRole::Require,
        );
    }

    /// Record local named exports without a re-export source.
    pub(super) fn record_local_named_exports_only(
        &mut self,
        specifiers: &[ExportSpecifier],
        resolver: &Resolver,
    ) {
        self.record_local_named_exports(specifiers, resolver);
    }

    /// Record re-exports from a source module at a resolved byte range.
    pub(super) fn record_reexports_from_source(
        &mut self,
        export: &NamedExport,
        source: &swc_ecma_ast::Str,
        source_span: ByteRange,
    ) {
        self.record_reexports(export, source, source_span);
    }

    fn record_local_named_exports(&mut self, specifiers: &[ExportSpecifier], resolver: &Resolver) {
        for specifier in specifiers {
            if let ExportSpecifier::Named(named) = specifier
                && !named.is_type_only
            {
                let original = module_export_name(&named.orig);
                let exported = named
                    .exported
                    .as_ref()
                    .map_or_else(|| original.clone(), module_export_name);
                if let swc_ecma_ast::ModuleExportName::Ident(ident) = &named.orig
                    && let Some(id) = resolver.function_id_for_expr(&Expr::Ident(ident.clone()))
                {
                    self.interface.add_function_export(exported.clone(), id);
                }
                self.interface
                    .add_export(exported, ModuleExport::Local { name: original });
            }
        }
    }

    fn record_reexports(
        &mut self,
        export: &NamedExport,
        source: &swc_ecma_ast::Str,
        source_span: ByteRange,
    ) {
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
        let span = source_span;
        let request = self.interface.add_request(
            span,
            ResolutionRequestKind::StaticImport,
            source.value.to_string_lossy(),
            ModuleRequestRole::ReExport {
                bindings: specifiers
                    .iter()
                    .map(|specifier| match specifier {
                        ExportSpecifier::Named(named) => ReExportBinding::new(
                            module_export_name(&named.orig),
                            named.exported.as_ref().map_or_else(
                                || module_export_name(&named.orig),
                                module_export_name,
                            ),
                            false,
                        ),
                        ExportSpecifier::Namespace(namespace) => ReExportBinding::new(
                            NAMESPACE_EXPORT.into(),
                            module_export_name(&namespace.name),
                            true,
                        ),
                        ExportSpecifier::Default(default) => ReExportBinding::new(
                            DEFAULT_EXPORT.into(),
                            default.exported.sym.to_smolstr(),
                            false,
                        ),
                    })
                    .collect(),
            },
        );
        for specifier in specifiers {
            match specifier {
                ExportSpecifier::Named(named) => {
                    let original = module_export_name(&named.orig);
                    let exported = named
                        .exported
                        .as_ref()
                        .map_or_else(|| original.clone(), module_export_name);
                    self.interface.add_export(
                        exported,
                        ModuleExport::ReExport {
                            request,
                            imported: original,
                        },
                    );
                }
                ExportSpecifier::Namespace(namespace) => self.interface.add_export(
                    module_export_name(&namespace.name),
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
    pub(super) fn record_export_all(&mut self, export: &ExportAll, source_span: ByteRange) {
        if export.type_only {
            return;
        }
        let span = source_span;
        let request = self.interface.add_request(
            span,
            ResolutionRequestKind::StaticImport,
            export.src.value.to_string_lossy(),
            ModuleRequestRole::StarExport,
        );
        self.interface.add_star_export(request);
    }

    /// Record the default export's supported function, local, or value shape.
    /// Returns true when a named local was recorded so the caller may visit
    /// the expression subtree if needed.
    pub(super) fn record_default_expr(&mut self, export: &ExportDefaultExpr, resolver: &Resolver) {
        if let Expr::Ident(ident) = &*export.expr {
            if let Some(id) = resolver.function_id_for_expr(&Expr::Ident(ident.clone())) {
                self.interface.add_function_export("default", id);
            }
            self.interface.add_export(
                "default",
                ModuleExport::Local {
                    name: ident.sym.to_smolstr(),
                },
            );
        } else {
            if let Some(id) = resolver.function_id_for_span(export.expr.span()) {
                self.interface.add_function_export("default", id);
            }
            self.interface.add_export("default", ModuleExport::Value);
        }
    }

    /// Record a default declaration without claiming an anonymous value is a
    /// named local when no stable identity exists.
    pub(super) fn record_default_decl(&mut self, export: &ExportDefaultDecl, resolver: &Resolver) {
        match &export.decl {
            DefaultDecl::Fn(function) => {
                if let Some(ident) = &function.ident {
                    self.record_local(ident.sym.to_string());
                    if let Some(id) = resolver.function_id_for_expr(&Expr::Ident(ident.clone())) {
                        self.interface.add_function_export("default", id);
                    }
                    self.interface.add_export(
                        "default",
                        ModuleExport::Local {
                            name: ident.sym.to_smolstr(),
                        },
                    );
                } else {
                    if let Some(id) = resolver.function_id_for_span(function.function.span()) {
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
                            name: ident.sym.to_smolstr(),
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
    }

    /// Translate supported CommonJS assignment shapes into interface entries.
    pub(super) fn record_commonjs_export(
        &mut self,
        assignment: &swc_ecma_ast::AssignExpr,
        resolver: &Resolver,
    ) {
        use swc_ecma_ast::AssignOp;

        if assignment.op != AssignOp::Assign {
            return;
        }
        let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(member)) =
            &assignment.left
        else {
            return;
        };
        let prop = crate::analysis::syntax::member_property_name(&member.prop);
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
            || crate::analysis::syntax::member_property_name(&parent.prop).as_deref()
                != Some(COMMONJS_EXPORTS)
        {
            return;
        }
        let Some(property) = prop else {
            self.interface.mark_unknown_exports();
            return;
        };
        self.record_commonjs_property_export(assignment, Some(property), resolver);
    }

    fn record_module_exports_assignment(
        &mut self,
        assignment: &swc_ecma_ast::AssignExpr,
        resolver: &Resolver,
    ) {
        if self.interface.has_exports() {
            self.interface.mark_unknown_exports();
            return;
        }
        if let Expr::Object(object) = &*assignment.right {
            let Some(entries) = Self::collect_commonjs_export_entries(object) else {
                self.interface.mark_unknown_exports();
                return;
            };
            self.interface.add_export("default", ModuleExport::Value);
            for entry in entries {
                if let Some(span) = entry.value_span {
                    add_function_export_if_span(&mut self.interface, &entry.name, span, resolver);
                }
                if let Some(ref local) = entry.local {
                    add_function_export_if_name(
                        &mut self.interface,
                        &entry.name,
                        local,
                        assignment.span(),
                        resolver,
                    );
                }
                if let Some(value) = entry.static_value {
                    self.interface.add_static_string(entry.name.clone(), value);
                }
                self.interface.add_export(
                    entry.name,
                    entry
                        .local
                        .map_or(ModuleExport::Value, |n| ModuleExport::Local { name: n }),
                );
            }
        } else {
            if let Some(id) = resolver.function_id_for_span(assignment.right.span()) {
                self.interface.add_function_export("default", id);
            }
            self.interface.add_export("default", ModuleExport::Value);
        }
    }

    fn record_commonjs_property_export(
        &mut self,
        assignment: &swc_ecma_ast::AssignExpr,
        property: Option<SmolStr>,
        resolver: &Resolver,
    ) {
        let Some(property) = property else {
            self.interface.mark_unknown_exports();
            return;
        };
        let export = match &*assignment.right {
            Expr::Ident(ident) => {
                add_function_export_if_name(
                    &mut self.interface,
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
                add_function_export_if_expr(&mut self.interface, &property, expr, resolver);
                if let Expr::Lit(swc_ecma_ast::Lit::Str(value)) = expr {
                    self.interface
                        .add_static_string(property.clone(), value.value.to_string_lossy());
                }
                ModuleExport::Value
            }
        };
        self.interface.add_export(property, export);
    }

    fn collect_commonjs_export_entries(object: &ObjectLit) -> Option<Vec<CommonJsExportEntry>> {
        object
            .props
            .iter()
            .map(|prop| match prop {
                PropOrSpread::Prop(prop) => match &**prop {
                    Prop::KeyValue(value) => {
                        let name = property_name(&value.key)?;
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
                        name: property_name(&getter.key)?,
                        local: None,
                        value_span: None,
                        static_value: None,
                    }),
                    Prop::Setter(setter) => Some(CommonJsExportEntry {
                        name: property_name(&setter.key)?,
                        local: None,
                        value_span: None,
                        static_value: None,
                    }),
                    Prop::Method(method) => Some(CommonJsExportEntry {
                        name: property_name(&method.key)?,
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

/// Metadata for one property in a `module.exports = { ... }` literal,
/// extracted exactly once from the AST.
struct CommonJsExportEntry {
    /// Property/export name.
    name: SmolStr,
    /// Local binding name when the export is a simple identity re-export.
    local: Option<SmolStr>,
    /// Span of the value expression, used for function-identity lookup.
    value_span: Option<Span>,
    /// Static string value when the RHS is a string literal.
    static_value: Option<String>,
}

fn is_commonjs_name(expr: &swc_ecma_ast::Expr, name: &str, resolver: &Resolver) -> bool {
    matches!(expr, Expr::Ident(ident) if resolver.is_unshadowed_commonjs_name(ident, name))
}

fn add_function_export_if_name(
    interface: &mut crate::analysis::module::ModuleInterface,
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
    interface: &mut crate::analysis::module::ModuleInterface,
    export: &str,
    expr: &Expr,
    resolver: &Resolver,
) {
    add_function_export_if_span(interface, export, expr.span(), resolver);
}

fn add_function_export_if_span(
    interface: &mut crate::analysis::module::ModuleInterface,
    export: &str,
    span: Span,
    resolver: &Resolver,
) {
    if let Some(id) = resolver.function_id_for_span(span) {
        interface.add_function_export(export, id);
    }
}
