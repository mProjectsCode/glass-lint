use glass_lint_datastructures::SymbolPath;
use smol_str::{SmolStr, ToSmolStr};

use crate::analysis::{
    scope::{
        frozen_assignments::{BindingResolution, BindingResolutionStatus},
        provenance_to_const_value,
        query::{
            BindingKey, BindingProvenance, FrozenScopeGraph, Ident, IdentValueSeed, MemberExpr,
            ScopeKind, Span, SymbolCallProvenance, SymbolMemberProvenance, constant,
        },
    },
    syntax::{expression_name, member_root_identifier},
};

struct ResolvedIdentBinding<'a> {
    dynamic_lookup: bool,
    resolution: BindingResolution<'a>,
    binding: Option<BindingKey>,
}

impl FrozenScopeGraph {
    fn symbol_path_provenance(
        &self,
        target: &SymbolPath,
        check_path: &SymbolPath,
        span: Span,
    ) -> SymbolCallProvenance {
        if check_path.is_root()
            && let Some(root_segment) = check_path.first_segment()
            && self.is_global(root_segment)
        {
            SymbolCallProvenance::Global {
                name: root_segment.to_smolstr(),
            }
        } else {
            self.module_export_for_chain(&target.to_string(), span)
                .unwrap_or(SymbolCallProvenance::Local)
        }
    }

    pub(in crate::analysis) fn call_provenance(
        &self,
        name: &str,
        span: Span,
    ) -> SymbolCallProvenance {
        if self.has_dynamic_lookup_at(span) {
            return SymbolCallProvenance::Local;
        }
        let resolution = self.binding_resolution_at(name, span);
        self.call_provenance_from_resolution(name, span, resolution)
    }

    fn call_provenance_from_resolution(
        &self,
        name: &str,
        span: Span,
        resolution: BindingResolution<'_>,
    ) -> SymbolCallProvenance {
        match resolution.preferred_witness() {
            Some(BindingProvenance::ModuleExport { module, export }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.clone(),
                }
            }
            Some(BindingProvenance::DefaultImport { module }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: "default".into(),
                }
            }
            Some(BindingProvenance::ValueAlias { target }) => {
                let Some(path) = self.symbol_path(target) else {
                    return SymbolCallProvenance::Local;
                };
                let root = path.without_bind_suffix().unwrap_or_else(|| path.clone());
                self.symbol_path_provenance(&path, &root, span)
            }
            Some(BindingProvenance::BoundCallable { target, .. }) => {
                let Some(path) = self.symbol_path(target) else {
                    return SymbolCallProvenance::Local;
                };
                self.symbol_path_provenance(&path, &path, span)
            }
            Some(BindingProvenance::BoundModuleCallable { module, export, .. }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.clone(),
                }
            }
            Some(
                BindingProvenance::Local
                | BindingProvenance::ModuleNamespace { .. }
                | BindingProvenance::ConstructedInstance { .. }
                | BindingProvenance::ReturnedObject { .. }
                | BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_),
            ) => SymbolCallProvenance::Local,
            None if resolution.status() == BindingResolutionStatus::Absent
                && self.is_global(name) =>
            {
                SymbolCallProvenance::Global {
                    name: name.to_smolstr(),
                }
            }
            None => SymbolCallProvenance::Local,
        }
    }

    pub(in crate::analysis) fn ident_value_seed(&self, ident: &Ident) -> IdentValueSeed {
        let ResolvedIdentBinding {
            dynamic_lookup,
            resolution,
            binding,
        } = self.ident_binding_seed(ident);
        let constant = if dynamic_lookup {
            constant::ConstValue::Unknown
        } else {
            let resolve = |key| self.resolve_name_id(key).map(SmolStr::new);
            resolution
                .preferred_witness()
                .map_or(constant::ConstValue::Unknown, |provenance| {
                    provenance_to_const_value(provenance, &resolve)
                })
        };
        // PERF: Identifier resolution is the hottest operation on minified
        // bundles. Keep every projection on the same joined-binding result;
        // calling the public convenience queries here would repeat scope,
        // assignment, and dynamic-lookup searches for each projection.
        IdentValueSeed {
            call: if dynamic_lookup {
                SymbolCallProvenance::Local
            } else {
                self.call_provenance_from_resolution(ident.sym.as_ref(), ident.span, resolution)
            },
            rooted_chain: if dynamic_lookup {
                None
            } else {
                self.callable_member_chain_from_resolution(resolution)
                    .and_then(|path| self.name_path(&path))
            },
            binding,
            constant,
            bound_arguments: resolution.preferred_witness().and_then(
                |provenance| match provenance {
                    BindingProvenance::BoundCallable {
                        bound_arguments, ..
                    }
                    | BindingProvenance::BoundModuleCallable {
                        bound_arguments, ..
                    } => Some(bound_arguments.clone()),
                    _ => None,
                },
            ),
        }
    }

    fn ident_binding_seed(&self, ident: &Ident) -> ResolvedIdentBinding<'_> {
        let Some(use_scope) = self.scope_at(ident.span) else {
            return ResolvedIdentBinding {
                dynamic_lookup: true,
                resolution: BindingResolution::absent(),
                binding: None,
            };
        };
        let dynamic_lookup = self.scope_or_ancestor_has_kind(use_scope, ScopeKind::Dynamic)
            || self.has_prior_eval(use_scope, ident.span);
        let Some(name) = self.name_id(ident.sym.as_ref()) else {
            return ResolvedIdentBinding {
                dynamic_lookup,
                resolution: BindingResolution::absent(),
                binding: None,
            };
        };
        let Some((binding_scope, resolution)) = self.resolve_binding(name, use_scope, ident.span)
        else {
            return ResolvedIdentBinding {
                dynamic_lookup,
                resolution: BindingResolution::absent(),
                binding: None,
            };
        };
        let binding = self.lexical_binding_key(binding_scope, name, ident.span);
        ResolvedIdentBinding {
            dynamic_lookup,
            resolution,
            binding,
        }
    }

    pub(in crate::analysis) fn contextual_member_property_name(
        &self,
        member: &MemberExpr,
    ) -> Option<SmolStr> {
        constant::contextual_member_property_name(&member.prop, self)
    }

    pub(in crate::analysis) fn contextual_member_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<SymbolPath> {
        let object = expression_name(&member.obj)?;
        Some(object.append_chain(&self.contextual_member_property_name(member)?))
    }

    /// Resolve the seed's rooted identity from the preferred witness only.
    ///
    /// Unlike [`rooted_witness_path`], this deliberately fails closed for a
    /// joined binding whose preferred witness is not rooted-available:
    /// skipping to a later rooted alternative would let an ambiguous binding
    /// claim a rooted identity, which the flow projector's value identity
    /// would then accept as a witness.
    fn callable_member_chain_from_resolution(
        &self,
        resolution: BindingResolution<'_>,
    ) -> Option<SymbolPath> {
        match resolution.preferred_witness()? {
            BindingProvenance::ValueAlias { target } if self.rooted_path_available(target) => self
                .symbol_path(target)
                .and_then(|path| path.without_bind_suffix().or(Some(path))),
            BindingProvenance::BoundCallable { target, .. }
                if self.rooted_path_available(target) =>
            {
                self.symbol_path(target)
            }
            BindingProvenance::BoundModuleCallable { .. } => None,
            BindingProvenance::ReturnedObject { source } if self.rooted_path_available(source) => {
                self.symbol_path(source)
            }
            _ => None,
        }
    }

    pub(in crate::analysis) fn module_export_for_chain(
        &self,
        chain: &str,
        span: Span,
    ) -> Option<SymbolCallProvenance> {
        let (root, export) = chain.split_once('.')?;
        match self.definite_binding_at(root, span)? {
            BindingProvenance::DefaultImport { module }
            | BindingProvenance::ModuleNamespace { module } => {
                Some(SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.to_smolstr(),
                })
            }
            _ => None,
        }
    }

    pub(in crate::analysis) fn member_call_provenance_for_chain(
        &self,
        member: &MemberExpr,
        chain: &SymbolPath,
    ) -> Option<SymbolMemberProvenance> {
        if self.has_dynamic_lookup_at(member.span) {
            return None;
        }
        if let Some((module, member)) = self.module_member_for_member(member) {
            return Some(SymbolMemberProvenance::ModuleNamespace { module, member });
        }
        let root = member_root_identifier(member)?;
        if chain.first_segment().is_none_or(|s| s != root.sym.as_ref()) {
            return None;
        }
        let member = chain
            .as_view()
            .tail_after(1)?
            .segments()
            .iter()
            .map(SmolStr::as_str)
            .collect::<Vec<_>>()
            .join(".");
        match self.definite_binding_at(root.sym.as_ref(), root.span) {
            Some(
                BindingProvenance::DefaultImport { module }
                | BindingProvenance::ModuleNamespace { module },
            ) => Some(SymbolMemberProvenance::ModuleNamespace {
                module: module.clone(),
                member: member.to_smolstr(),
            }),
            _ => None,
        }
    }
}
