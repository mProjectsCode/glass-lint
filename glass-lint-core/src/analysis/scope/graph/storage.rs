use glass_lint_datastructures::NameId;
use swc_common::{BytePos, Span};

use crate::analysis::{
    model::scope::{
        BindingId, BindingKey, BindingProvenance, BindingVersion, FunctionId, ScopeId, ScopeKind,
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
    pub(super) fn binding_with_scope_at(
        &self,
        name: NameId,
        mut scope: ScopeId,
    ) -> Option<(ScopeId, &BindingProvenance)> {
        loop {
            if let Some(binding) = self.scopes.scope_binding(scope, name) {
                return Some((scope, binding));
            }
            scope = self.scopes.scope_parent(scope)?;
        }
    }

    pub(super) fn parameter_alias_for_scope(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<&BindingProvenance> {
        self.bindings.parameter_alias_for_scope(scope, name)
    }

    pub(super) fn enclosing_function_at(&self, scope: ScopeId) -> FunctionId {
        let mut current = Some(scope);
        while let Some(scope) = current {
            if let Some(function) = self.bindings.function_for_scope(scope) {
                return function;
            }
            current = self.scopes.scope_parent(scope);
        }
        FunctionId::new(0)
    }
}

pub(super) struct ScopeReadView<'a, M> {
    pub(super) data: &'a ScopeData<M>,
    pub(super) scope_shape_valid: bool,
}

impl<'a, M> ScopeReadView<'a, M> {
    pub(super) fn scope_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.data.scopes.scope_parent(scope)
    }

    pub(super) fn scope_kind(&self, scope: ScopeId) -> Option<ScopeKind> {
        self.data.scopes.scope_kind(scope)
    }

    pub(super) fn scope_span(&self, scope: ScopeId) -> Option<Span> {
        self.data.scopes.scope_span(scope)
    }

    pub(super) fn scope_at(&self, span: Span) -> Option<ScopeId> {
        self.data.scopes.scope_at(span, self.scope_shape_valid)
    }

    pub(super) fn enclosing_function_at(&self, scope: ScopeId) -> FunctionId {
        self.data.enclosing_function_at(scope)
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
                self.enclosing_function_at(scope),
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

    pub(super) fn reassigned_between(
        &self,
        scope: ScopeId,
        name: NameId,
        start: BytePos,
        end: BytePos,
    ) -> bool {
        self.data
            .bindings
            .reassigned_between(scope, name, start, end)
    }

    pub(super) fn function_span(&self, function: FunctionId) -> Option<Span> {
        self.data.bindings.function_span(function)
    }

    pub(super) fn function_containing(&self, span: Span) -> Option<FunctionId> {
        self.data.bindings.function_containing(span)
    }

    pub(super) fn function_binding(&self, scope: ScopeId, name: NameId) -> Option<FunctionId> {
        self.data.bindings.function_binding(scope, name)
    }

    pub(super) fn function_alias(&self, scope: ScopeId, name: NameId) -> Option<FunctionId> {
        self.data.bindings.function_alias(scope, name)
    }
}
