use glass_lint_datastructures::NameId;
use hashbrown::HashMap;
use swc_common::{BytePos, Span};

use crate::analysis::{
    scope::model::{
        frozen_assignments::FrozenAssignmentIndex,
        id::{ScopeId, ScopedName},
        scope_index::LexicalScopeIndex,
        types::{AliasAssignment, BindingProvenance},
    },
    value::{BindingId, BindingVersion, FunctionId},
};

#[derive(Debug)]
pub(super) struct BindingIndex {
    pub(super) assignments: FrozenAssignmentIndex,
    pub(super) binding_ids: HashMap<ScopedName, BindingId>,
    pub(super) function_ids: Vec<Option<FunctionId>>,
    pub(super) function_bindings: HashMap<ScopedName, FunctionId>,
    pub(super) function_aliases: HashMap<ScopedName, FunctionId>,
    pub(super) parameter_aliases: HashMap<(FunctionId, NameId), BindingProvenance>,
}

impl BindingIndex {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        assignments: FrozenAssignmentIndex,
        binding_ids: HashMap<ScopedName, BindingId>,
        function_ids: Vec<Option<FunctionId>>,
        function_bindings: HashMap<ScopedName, FunctionId>,
        function_aliases: HashMap<ScopedName, FunctionId>,
        parameter_aliases: HashMap<(FunctionId, NameId), BindingProvenance>,
    ) -> Self {
        Self {
            assignments,
            binding_ids,
            function_ids,
            function_bindings,
            function_aliases,
            parameter_aliases,
        }
    }

    pub(super) fn assignment_at(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> Option<&AliasAssignment> {
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
        self.parameter_aliases.get(&(function, name))
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
                function.and_then(|f| scopes.scope_span(ScopeId::from(idx)).map(|span| (f, span)))
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
