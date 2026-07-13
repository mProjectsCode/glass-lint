//! Pattern projection helpers shared by declaration and assignment collection.
//!
//! Destructuring is followed only when every traversed segment has an explicit,
//! static provenance. Rest elements and dynamic property forms intentionally
//! stop the projection rather than guessing about the remaining value.

use swc_common::Span;
use swc_ecma_ast::{ObjectPatProp, Pat};

use super::super::super::syntax::prop_name;
use super::{AliasCollector, BindingProvenance};

impl AliasCollector {
    /// Record aliases introduced by a destructuring declaration.
    ///
    /// This deliberately stops at unsupported pattern forms. A partial
    /// projection would make a later use look more precise than the source
    /// warrants, so callers should leave the binding unresolved instead.
    pub(super) fn collect_value_aliases(&mut self, pat: &Pat, target: &str, scope: usize) {
        match pat {
            Pat::Ident(ident) => self.insert(
                scope,
                ident.id.sym.to_string(),
                BindingProvenance::ValueAlias {
                    target: target.to_string().into(),
                },
            ),
            Pat::Object(object) => {
                for prop in &object.props {
                    match prop {
                        ObjectPatProp::KeyValue(key_value) => {
                            if let Some(property) = prop_name(&key_value.key) {
                                self.collect_value_aliases(
                                    &key_value.value,
                                    &format!("{target}.{property}"),
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
                                    target: format!("{target}.{property}").into(),
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
        target: &str,
        span: Span,
        scope: usize,
    ) {
        match pat {
            Pat::Ident(ident) => self.record_assignment(
                span,
                scope,
                ident.id.sym.to_string(),
                BindingProvenance::ValueAlias {
                    target: target.to_string().into(),
                },
            ),
            Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        ObjectPatProp::KeyValue(key_value) => {
                            if let Some(name) = prop_name(&key_value.key) {
                                self.collect_assignment_aliases(
                                    &key_value.value,
                                    &format!("{target}.{name}"),
                                    span,
                                    scope,
                                );
                            }
                        }
                        ObjectPatProp::Assign(assign) => {
                            let name = assign.key.sym.to_string();
                            self.record_assignment(
                                span,
                                scope,
                                name.clone(),
                                BindingProvenance::ValueAlias {
                                    target: format!("{target}.{name}").into(),
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
    pub(super) fn collect_require_aliases(&mut self, pat: &Pat, module: String, scope: usize) {
        match pat {
            Pat::Ident(ident) => {
                self.insert(
                    scope,
                    ident.id.sym.to_string(),
                    BindingProvenance::ModuleNamespace { module },
                );
            }
            Pat::Object(object) => {
                for prop in &object.props {
                    match prop {
                        ObjectPatProp::KeyValue(key_value) => {
                            if let Some(imported) = prop_name(&key_value.key) {
                                self.collect_require_export_alias(
                                    &key_value.value,
                                    &module,
                                    &imported,
                                    scope,
                                );
                            }
                        }
                        ObjectPatProp::Assign(assign) => {
                            let local = assign.key.sym.to_string();
                            self.insert(
                                scope,
                                local.clone(),
                                BindingProvenance::ModuleExport {
                                    module: module.clone(),
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
        scope: usize,
    ) {
        if let Pat::Ident(local) = pat {
            self.insert(
                scope,
                local.id.sym.to_string(),
                BindingProvenance::ModuleExport {
                    module: module.to_string(),
                    export: export.to_string(),
                },
            );
        }
    }
}

pub fn contains(outer: Span, inner: Span) -> bool {
    outer.lo <= inner.lo && outer.hi >= inner.hi
}

pub fn member_prefix_ends(chain: &str) -> impl Iterator<Item = usize> + '_ {
    // Visit the complete chain first, then progressively shorter prefixes so a
    // direct write to `a.b.c` takes precedence over an older `a.b` write.
    std::iter::once(chain.len()).chain(chain.rmatch_indices('.').map(|(index, _)| index))
}
