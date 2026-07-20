//! Pattern projection helpers shared by declaration and assignment collection.
//!
//! Destructuring is followed only when every traversed segment has an explicit,
//! static provenance. Rest elements and dynamic property forms intentionally
//! stop the projection rather than guessing about the remaining value.

use smol_str::{SmolStr, ToSmolStr};
use swc_common::Span;
use swc_ecma_ast::{ObjectPatProp, Pat};

use super::{
    super::super::syntax::property_name, BindingProvenance, LexicalScopeCollector, ScopeId,
};
use crate::analysis::value::NamePath;

impl LexicalScopeCollector<'_> {
    /// Record aliases introduced by a destructuring declaration.
    ///
    /// This deliberately stops at unsupported pattern forms. A partial
    /// projection would make a later use look more precise than the source
    /// warrants, so callers should leave the binding unresolved instead.
    pub(super) fn collect_value_aliases(&mut self, pat: &Pat, target: &NamePath, scope: ScopeId) {
        match pat {
            Pat::Ident(ident) => self.insert(
                scope,
                ident.id.sym.to_string(),
                BindingProvenance::ValueAlias {
                    target: target.clone(),
                },
            ),
            Pat::Object(object) => {
                for prop in &object.props {
                    match prop {
                        ObjectPatProp::KeyValue(key_value) => {
                            if let Some(property) = property_name(&key_value.key) {
                                self.collect_value_aliases(
                                    &key_value.value,
                                    &self.append_name_path(target, &property).unwrap_or_default(),
                                    scope,
                                );
                            }
                        }
                        ObjectPatProp::Assign(assign) => {
                            let property = assign.key.sym.to_string();
                            self.insert(
                                scope,
                                property.clone(),
                                BindingProvenance::ValueAlias {
                                    target: self
                                        .append_name_path(target, &property)
                                        .unwrap_or_default(),
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

    /// Record aliases introduced by a destructuring assignment.
    pub(super) fn collect_assignment_aliases(
        &mut self,
        pat: &Pat,
        target: &super::super::super::value::NamePath,
        span: Span,
        scope: ScopeId,
    ) {
        match pat {
            Pat::Ident(ident) => self.record_assignment(
                span,
                scope,
                ident.id.sym.as_ref(),
                BindingProvenance::ValueAlias {
                    target: target.clone(),
                },
            ),
            Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        ObjectPatProp::KeyValue(key_value) => {
                            if let Some(name) = property_name(&key_value.key) {
                                self.collect_assignment_aliases(
                                    &key_value.value,
                                    &self.append_name_path(target, &name).unwrap_or_default(),
                                    span,
                                    scope,
                                );
                            }
                        }
                        ObjectPatProp::Assign(assign) => {
                            let name = assign.key.sym.to_smolstr();
                            self.record_assignment(
                                span,
                                scope,
                                name.as_str(),
                                BindingProvenance::ValueAlias {
                                    target: self
                                        .append_name_path(target, &name)
                                        .unwrap_or_default(),
                                },
                            );
                        }
                        ObjectPatProp::Rest(_) => {}
                    }
                }
            }
            Pat::Assign(assign) => {
                self.collect_assignment_aliases(&assign.left, target, span, scope);
            }
            _ => {}
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
