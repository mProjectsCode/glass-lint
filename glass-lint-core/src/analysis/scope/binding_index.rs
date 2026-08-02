use glass_lint_datastructures::NameId;
use hashbrown::HashMap;
use swc_common::{BytePos, Span};

use crate::analysis::{
    model::scope::{BindingProvenance, ScopeId, ScopedName},
    scope::{
        frozen_assignments::{AssignmentAt, FrozenAssignmentIndex},
        scope_index::LexicalScopeIndex,
    },
    value::{BindingId, BindingVersion, FunctionId},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::analysis) struct ParameterAliasKey {
    function: FunctionId,
    name: NameId,
}

impl ParameterAliasKey {
    pub(in crate::analysis) fn new(function: FunctionId, name: NameId) -> Self {
        Self { function, name }
    }
}

#[derive(Debug)]
pub(in crate::analysis) struct BindingIndexParts {
    pub(in crate::analysis) assignments: FrozenAssignmentIndex,
    pub(in crate::analysis) binding_ids: HashMap<ScopedName, BindingId>,
    pub(in crate::analysis) function_ids: Vec<Option<FunctionId>>,
    pub(in crate::analysis) function_bindings: HashMap<ScopedName, FunctionId>,
    pub(in crate::analysis) function_aliases: HashMap<ScopedName, FunctionId>,
    pub(in crate::analysis) parameter_aliases: HashMap<ParameterAliasKey, BindingProvenance>,
}

#[derive(Debug)]
pub(super) struct BindingIndex {
    pub(super) assignments: FrozenAssignmentIndex,
    pub(super) binding_ids: HashMap<ScopedName, BindingId>,
    pub(super) function_ids: Vec<Option<FunctionId>>,
    pub(super) function_bindings: HashMap<ScopedName, FunctionId>,
    pub(super) function_aliases: HashMap<ScopedName, FunctionId>,
    pub(super) parameter_aliases: HashMap<ParameterAliasKey, BindingProvenance>,
}

impl BindingIndex {
    pub(super) fn from_parts(parts: BindingIndexParts) -> Self {
        Self {
            assignments: parts.assignments,
            binding_ids: parts.binding_ids,
            function_ids: parts.function_ids,
            function_bindings: parts.function_bindings,
            function_aliases: parts.function_aliases,
            parameter_aliases: parts.parameter_aliases,
        }
    }

    pub(super) fn assignment_at(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> AssignmentAt<'_> {
        self.assignments.latest_at(scope, name, span)
    }

    pub(super) fn binding_id_at(&self, scope: ScopeId, name: NameId) -> Option<BindingId> {
        self.binding_ids.get(&ScopedName::new(scope, name)).copied()
    }

    pub(super) fn parameter_alias_for(
        &self,
        function: FunctionId,
        name: NameId,
    ) -> Option<&BindingProvenance> {
        self.parameter_aliases
            .get(&ParameterAliasKey::new(function, name))
    }

    pub(super) fn reassigned_between(
        &self,
        scope: ScopeId,
        name: NameId,
        start: BytePos,
        end: BytePos,
    ) -> bool {
        self.assignments.changed_between(scope, name, start, end)
    }

    pub(super) fn binding_version(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> BindingVersion {
        self.assignments.version_at(scope, name, span)
    }

    pub(super) fn function_for_scope(&self, scope: ScopeId) -> Option<FunctionId> {
        self.function_ids.get(scope.index()).copied().flatten()
    }

    pub(super) fn function_spans<'a>(
        &'a self,
        scopes: &'a LexicalScopeIndex,
    ) -> impl Iterator<Item = (FunctionId, Span)> + 'a {
        self.function_ids
            .iter()
            .enumerate()
            .filter_map(move |(idx, function)| {
                function.and_then(|f| scopes.scope_span(ScopeId::new(idx)).map(|span| (f, span)))
            })
    }

    pub(super) fn function_binding(&self, scope: ScopeId, name: NameId) -> Option<FunctionId> {
        self.function_bindings
            .get(&ScopedName::new(scope, name))
            .copied()
    }

    pub(super) fn function_alias(&self, scope: ScopeId, name: NameId) -> Option<FunctionId> {
        self.function_aliases
            .get(&ScopedName::new(scope, name))
            .copied()
    }
}
