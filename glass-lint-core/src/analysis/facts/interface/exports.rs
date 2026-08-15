use smol_str::{SmolStr, ToSmolStr};
use swc_common::Spanned;
use swc_ecma_ast::{
    DefaultDecl, ExportAll, ExportDefaultDecl, ExportDefaultExpr, ExportNamedSpecifier,
    ExportSpecifier, Expr, NamedExport,
};

use crate::analysis::{
    model::module::{ModuleExport, ModuleInterface},
    resolution::Resolver,
    syntax::module_export_name,
};

impl ModuleInterface {
    /// The `(original, exported)` name pair of a named export specifier, with
    /// the exported name falling back to the original when no alias is given.
    fn original_exported_pair(specifier: &ExportNamedSpecifier) -> (SmolStr, SmolStr) {
        let original = module_export_name(&specifier.orig);
        let exported = specifier
            .exported
            .as_ref()
            .map_or_else(|| original.clone(), module_export_name);
        (original, exported)
    }

    pub(in crate::analysis::facts) fn record_export_decl(
        &mut self,
        declaration: &swc_ecma_ast::Decl,
        resolver: &mut Resolver,
    ) {
        match declaration {
            swc_ecma_ast::Decl::Class(class) => {
                self.add_export(
                    class.ident.sym.to_string(),
                    ModuleExport::Local {
                        name: class.ident.sym.to_smolstr(),
                    },
                );
            }
            swc_ecma_ast::Decl::Fn(function) => {
                if let Some(id) =
                    resolver.function_id_for_expr(&Expr::Ident(function.ident.clone()))
                {
                    self.add_function_export(function.ident.sym.to_string(), id);
                }
                self.add_export(
                    function.ident.sym.to_string(),
                    ModuleExport::Local {
                        name: function.ident.sym.to_smolstr(),
                    },
                );
            }
            swc_ecma_ast::Decl::Var(variable) => {
                // The export pass runs before the visitor descends into the
                // declarator (visit_export_decl calls record_export_decl first),
                // so it records the names itself here; the visitor's later
                // record_pattern_locals re-inserts the same names, which is a
                // no-op because add_local is idempotent.
                for declarator in &variable.decls {
                    let names = self.record_pattern_locals(&declarator.name);
                    for name in names {
                        if let swc_ecma_ast::Pat::Ident(binding) = &declarator.name
                            && let Some(id) =
                                resolver.function_id_for_expr(&Expr::Ident(binding.id.clone()))
                        {
                            self.add_function_export(name.clone(), id);
                        }
                        self.add_export(name.clone(), ModuleExport::Local { name });
                        if let swc_ecma_ast::Pat::Ident(binding) = &declarator.name {
                            let value_id = resolver.resolve_ident_id(&binding.id);
                            if let Some(value) = resolver.static_string_value(value_id) {
                                self.add_static_string(binding.id.sym.to_string(), value);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub(in crate::analysis::facts) fn record_local_named_exports(
        &mut self,
        specifiers: &[ExportSpecifier],
        resolver: &Resolver,
    ) {
        for specifier in specifiers {
            if let ExportSpecifier::Named(named) = specifier
                && !named.is_type_only
            {
                let (original, exported) = Self::original_exported_pair(named);
                if let swc_ecma_ast::ModuleExportName::Ident(ident) = &named.orig
                    && let Some(id) = resolver.function_id_for_expr(&Expr::Ident(ident.clone()))
                {
                    self.add_function_export(exported.clone(), id);
                }
                self.add_export(exported, ModuleExport::Local { name: original });
            }
        }
    }

    pub(in crate::analysis::facts) fn record_reexports(
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
        let request = self.add_reexport_request(span, source.value.to_string_lossy());
        for specifier in specifiers {
            match specifier {
                ExportSpecifier::Named(named) => {
                    let (original, exported) = Self::original_exported_pair(named);
                    self.add_export(
                        exported,
                        ModuleExport::ReExport {
                            request,
                            imported: original,
                        },
                    );
                }
                ExportSpecifier::Namespace(namespace) => self.add_export(
                    module_export_name(&namespace.name),
                    ModuleExport::Namespace { request },
                ),
                ExportSpecifier::Default(default) => self.add_export(
                    default.exported.sym.to_string(),
                    ModuleExport::ReExport {
                        request,
                        imported: "default".into(),
                    },
                ),
            }
        }
    }

    pub(in crate::analysis::facts) fn record_export_all(
        &mut self,
        export: &ExportAll,
        source_span: glass_lint_datastructures::ByteRange,
    ) {
        if export.type_only {
            return;
        }
        self.add_star_export_request(source_span, export.src.value.to_string_lossy());
    }

    pub(in crate::analysis::facts) fn record_default_expr(
        &mut self,
        export: &ExportDefaultExpr,
        resolver: &Resolver,
    ) {
        if let Expr::Ident(ident) = &*export.expr {
            if let Some(id) = resolver.function_id_for_expr(&Expr::Ident(ident.clone())) {
                self.add_function_export("default", id);
            }
            self.add_export(
                "default",
                ModuleExport::Local {
                    name: ident.sym.to_smolstr(),
                },
            );
        } else {
            if let Some(id) = resolver.function_id_for_span(export.expr.span()) {
                self.add_function_export("default", id);
            }
            self.add_export("default", ModuleExport::Value);
        }
    }

    pub(in crate::analysis::facts) fn record_default_decl(
        &mut self,
        export: &ExportDefaultDecl,
        resolver: &Resolver,
    ) {
        match &export.decl {
            DefaultDecl::Fn(function) => {
                if let Some(ident) = &function.ident {
                    self.add_local(ident.sym.to_string());
                    if let Some(id) = resolver.function_id_for_expr(&Expr::Ident(ident.clone())) {
                        self.add_function_export("default", id);
                    }
                    self.add_export(
                        "default",
                        ModuleExport::Local {
                            name: ident.sym.to_smolstr(),
                        },
                    );
                } else {
                    if let Some(id) = resolver.function_id_for_span(function.function.span()) {
                        self.add_function_export("default", id);
                    }
                    self.add_export("default", ModuleExport::Value);
                }
            }
            DefaultDecl::Class(class) => {
                if let Some(ident) = &class.ident {
                    self.add_local(ident.sym.to_string());
                    self.add_export(
                        "default",
                        ModuleExport::Local {
                            name: ident.sym.to_smolstr(),
                        },
                    );
                } else {
                    self.add_export("default", ModuleExport::Value);
                }
            }
            DefaultDecl::TsInterfaceDecl(_) => {
                self.add_export("default", ModuleExport::Unknown);
            }
        }
    }
}
