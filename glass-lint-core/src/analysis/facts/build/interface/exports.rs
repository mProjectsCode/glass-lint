use smol_str::ToSmolStr;
use swc_common::Spanned;
use swc_ecma_ast::{
    DefaultDecl, ExportAll, ExportDefaultDecl, ExportDefaultExpr, ExportSpecifier, Expr,
    NamedExport,
};

use crate::{
    analysis::{
        module::{
            ModuleExport, ModuleRequestRole, ReExportBinding, DEFAULT_EXPORT, NAMESPACE_EXPORT,
        },
        resolution::Resolver,
        syntax::{collect_pat_bindings, module_export_name},
    },
    project::ResolutionRequestKind,
};

use super::ModuleInterfaceBuilder;

impl ModuleInterfaceBuilder {
    pub(in crate::analysis::facts::build) fn record_export_decl(
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

    pub(in crate::analysis::facts::build) fn record_local_named_exports_only(
        &mut self,
        specifiers: &[ExportSpecifier],
        resolver: &Resolver,
    ) {
        self.record_local_named_exports(specifiers, resolver);
    }

    pub(in crate::analysis::facts::build) fn record_reexports_from_source(
        &mut self,
        export: &NamedExport,
        source: &swc_ecma_ast::Str,
        source_span: glass_lint_datastructures::ByteRange,
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
        source_span: glass_lint_datastructures::ByteRange,
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

    pub(in crate::analysis::facts::build) fn record_export_all(
        &mut self,
        export: &ExportAll,
        source_span: glass_lint_datastructures::ByteRange,
    ) {
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

    pub(in crate::analysis::facts::build) fn record_default_expr(
        &mut self,
        export: &ExportDefaultExpr,
        resolver: &Resolver,
    ) {
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

    pub(in crate::analysis::facts::build) fn record_default_decl(
        &mut self,
        export: &ExportDefaultDecl,
        resolver: &Resolver,
    ) {
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
}
