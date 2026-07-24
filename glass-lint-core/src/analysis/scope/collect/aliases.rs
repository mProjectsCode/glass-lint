//! Pattern projection helpers shared by declaration and assignment collection.
//!
//! Destructuring is followed only when every traversed segment has an explicit,
//! static provenance. Rest elements and dynamic property forms intentionally
//! stop the projection rather than guessing about the remaining value.

use glass_lint_datastructures::NamePath;
use smol_str::{SmolStr, ToSmolStr};
use swc_common::Span;
use swc_ecma_ast::{ObjectPatProp, Pat};

use crate::analysis::{
    scope::{
        BindingProvenance, ScopeCollector, ScopeId,
        collect::projection::{ProjectionError, project_destructuring},
    },
    syntax::property_name,
};

impl ScopeCollector<'_> {
    /// Record aliases introduced by a destructuring declaration.
    ///
    /// This deliberately stops at unsupported pattern forms. A partial
    /// projection would make a later use look more precise than the source
    /// warrants, so callers should leave the binding unresolved instead.
    pub(super) fn collect_value_aliases(&mut self, pat: &Pat, target: &NamePath, scope: ScopeId) {
        let result = {
            let append = |path: &NamePath, segment: &str| self.append_name_path(path, segment);
            project_destructuring(pat, target, false, &append)
        };
        match result {
            Ok(bindings) => {
                for (name, path) in bindings {
                    self.insert(scope, name, BindingProvenance::ValueAlias { target: path });
                }
            }
            Err(ProjectionError::Unsupported) => {}
            Err(ProjectionError::Exhausted) => {
                self.name_exhausted = true;
            }
        }
    }

    /// Record aliases introduced by a destructuring assignment.
    pub(super) fn collect_assignment_aliases(
        &mut self,
        pat: &Pat,
        target: &NamePath,
        span: Span,
        scope: ScopeId,
    ) {
        let result = {
            let append = |path: &NamePath, segment: &str| self.append_name_path(path, segment);
            project_destructuring(pat, target, true, &append)
        };
        match result {
            Ok(bindings) => {
                for (name, path) in bindings {
                    self.record_assignment(
                        span,
                        scope,
                        name.as_str(),
                        BindingProvenance::ValueAlias { target: path },
                    );
                }
            }
            Err(ProjectionError::Unsupported) => {}
            Err(ProjectionError::Exhausted) => {
                self.name_exhausted = true;
            }
        }
    }

    /// Record CommonJS namespace and named-export aliases from a `require`.
    pub(super) fn collect_require_aliases(&mut self, pat: &Pat, module: SmolStr, scope: ScopeId) {
        match pat {
            Pat::Ident(ident) => {
                self.insert(
                    scope,
                    ident.id.sym.to_smolstr(),
                    BindingProvenance::ModuleNamespace { module },
                );
            }
            Pat::Object(object) => {
                for prop in &object.props {
                    match prop {
                        ObjectPatProp::KeyValue(key_value) => {
                            if let Some(imported) = property_name(&key_value.key) {
                                self.collect_require_export_alias(
                                    &key_value.value,
                                    &module,
                                    &imported,
                                    scope,
                                );
                            }
                        }
                        ObjectPatProp::Assign(assign) => {
                            let local = assign.key.sym.to_smolstr();
                            self.insert(
                                scope,
                                local.clone(),
                                BindingProvenance::ModuleExport {
                                    module: module.as_str().into(),
                                    export: local,
                                },
                            );
                        }
                        ObjectPatProp::Rest(_) => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_require_export_alias(
        &mut self,
        pat: &Pat,
        module: &str,
        export: &str,
        scope: ScopeId,
    ) {
        if let Pat::Ident(local) = pat {
            self.insert(
                scope,
                local.id.sym.to_smolstr(),
                BindingProvenance::ModuleExport {
                    module: module.into(),
                    export: export.into(),
                },
            );
        }
    }
}

pub(in crate::analysis::scope) fn contains(outer: Span, inner: Span) -> bool {
    outer.lo <= inner.lo && outer.hi >= inner.hi
}
