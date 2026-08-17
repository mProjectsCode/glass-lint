use glass_lint_datastructures::NameId;
use swc_common::Span;

use crate::analysis::{
    model::scope::{
        BindingId, BindingKey, BindingProvenance, BindingVersion, FunctionId, ScopeId,
    },
    scope::{
        binding_index::BindingIndex, frozen_assignments::AssignmentAt, name_env::NameEnvironment,
        scope_index::LexicalScopeIndex,
    },
};

#[derive(Debug)]
pub(super) struct ScopeData<M> {
    pub(super) names: NameEnvironment,
    pub(super) scopes: LexicalScopeIndex,
    pub(super) bindings: BindingIndex,
    pub(super) mutations: M,
}

impl<M> ScopeData<M> {
    /// Yield a scope and then each ancestor scope up to the root.
    pub(super) fn ancestors(&self, scope: ScopeId) -> impl Iterator<Item = ScopeId> + '_ {
        std::iter::successors(Some(scope), |scope| self.scopes.scope_parent(*scope))
    }

    pub(super) fn binding_with_scope_at(
        &self,
        name: NameId,
        scope: ScopeId,
    ) -> Option<(ScopeId, &BindingProvenance)> {
        self.ancestors(scope).find_map(|scope| {
            self.scopes
                .scope_binding(scope, name)
                .map(|binding| (scope, binding))
        })
    }

    pub(super) fn parameter_alias_for_scope(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<&BindingProvenance> {
        self.bindings.parameter_alias_for_scope(scope, name)
    }

    pub(super) fn enclosing_function_at(&self, scope: ScopeId) -> FunctionId {
        self.ancestors(scope)
            .find_map(|scope| self.bindings.function_for_scope(scope))
            .unwrap_or(FunctionId::new(0))
    }
}

pub(super) struct ScopeReadView<'a, M> {
    pub(super) data: &'a ScopeData<M>,
    pub(super) scope_shape_valid: bool,
}

impl<'a, M> ScopeReadView<'a, M> {
    pub(super) fn scope_at(&self, span: Span) -> Option<ScopeId> {
        self.data.scopes.scope_at(span, self.scope_shape_valid)
    }

    pub(super) fn nearest_binding_at(
        &self,
        name: NameId,
        span: Span,
    ) -> Option<(ScopeId, &'a BindingProvenance)> {
        self.data.binding_with_scope_at(name, self.scope_at(span)?)
    }

    pub(super) fn parameter_alias_for_scope(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<&'a BindingProvenance> {
        self.data.parameter_alias_for_scope(scope, name)
    }

    pub(super) fn assignment_at(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> AssignmentAt<'a> {
        self.data.bindings.assignment_at(scope, name, span)
    }

    pub(super) fn binding_id_at(&self, scope: ScopeId, name: NameId) -> Option<BindingId> {
        self.data.bindings.binding_id_at(scope, name)
    }

    /// Build a stable key for a name, using a global root when unbound.
    ///
    /// Shared by the collection and frozen query phases so the lexical-key
    /// construction cannot drift between them.
    pub(super) fn binding_key_for_name(&self, name: &str, span: Span) -> Option<BindingKey> {
        let Some(name_id) = self.data.names.name_id(name) else {
            return Some(BindingKey::global(name));
        };
        if let Some((scope, _)) = self.nearest_binding_at(name_id, span) {
            return Some(BindingKey::lexical(
                self.data.enclosing_function_at(scope),
                self.binding_id_at(scope, name_id)?,
                self.binding_version(scope, name_id, span),
            ));
        }
        Some(BindingKey::global(name))
    }

    pub(super) fn binding_version(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> BindingVersion {
        self.data.bindings.binding_version(scope, name, span)
    }
}
