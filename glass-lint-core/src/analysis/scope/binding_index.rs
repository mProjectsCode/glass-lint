use glass_lint_datastructures::NameId;
use hashbrown::HashMap;
use swc_common::{BytePos, Span};

use crate::analysis::{
    model::scope::{
        AliasAssignment, BindingProvenance, LexicalScope, ScopeId, ScopeKind, ScopedName,
    },
    scope::{
        frozen_assignments::{AssignmentAt, FrozenAssignmentIndex},
        scope_index::LexicalScopeIndex,
    },
    value::{BindingId, BindingVersion, FunctionId},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ParameterAliasKey {
    function: FunctionId,
    name: NameId,
}

impl ParameterAliasKey {
    fn new(function: FunctionId, name: NameId) -> Self {
        Self { function, name }
    }
}

/// Collector-side binding inputs consumed by the freeze transition.
#[derive(Debug)]
pub(super) struct BindingIndexInput {
    pub(super) assignments: Vec<AliasAssignment>,
    pub(super) binding_ids: HashMap<ScopedName, BindingId>,
    pub(super) function_ids: Vec<Option<FunctionId>>,
    pub(super) function_bindings: HashMap<ScopedName, ScopeId>,
    pub(super) function_aliases: HashMap<ScopedName, ScopeId>,
    pub(super) parameter_aliases: HashMap<ScopedName, BindingProvenance>,
}

#[derive(Debug)]
pub(super) struct BindingIndex {
    assignments: FrozenAssignmentIndex,
    binding_ids: HashMap<ScopedName, BindingId>,
    function_ids: Vec<Option<FunctionId>>,
    function_bindings: HashMap<ScopedName, FunctionId>,
    function_aliases: HashMap<ScopedName, FunctionId>,
    parameter_aliases: HashMap<ParameterAliasKey, BindingProvenance>,
}

impl From<BindingIndexInput> for BindingIndex {
    fn from(input: BindingIndexInput) -> Self {
        let BindingIndexInput {
            assignments,
            binding_ids,
            function_ids,
            function_bindings,
            function_aliases,
            parameter_aliases,
        } = input;
        let function_bindings = function_bindings
            .into_iter()
            .filter_map(|(binding, scope)| {
                function_ids
                    .get(scope.index())
                    .and_then(|&function| function)
                    .map(|function| (binding, function))
            })
            .collect();
        let function_aliases = function_aliases
            .into_iter()
            .filter_map(|(name, scope)| {
                function_ids
                    .get(scope.index())
                    .and_then(|&function| function)
                    .map(|function| (name, function))
            })
            .collect();
        let parameter_aliases = parameter_aliases
            .into_iter()
            .filter_map(|(name, provenance)| {
                function_ids
                    .get(name.scope().index())
                    .and_then(|&function| function)
                    .map(|function| (ParameterAliasKey::new(function, name.name()), provenance))
            })
            .collect();
        Self {
            assignments: FrozenAssignmentIndex::from_assignments(assignments),
            binding_ids,
            function_ids,
            function_bindings,
            function_aliases,
            parameter_aliases,
        }
    }
}

impl BindingIndex {
    /// Allocate stable binding and function IDs over the lexical scopes.
    pub(super) fn allocate_ids(
        scopes: &[LexicalScope],
    ) -> (HashMap<ScopedName, BindingId>, Vec<Option<FunctionId>>) {
        let mut binding_ids = HashMap::new();
        let mut next_binding = 0u32;
        for (scope, lexical_scope) in scopes.iter().enumerate() {
            let scope = ScopeId::new(scope);
            for name in lexical_scope.binding_names() {
                binding_ids.insert(ScopedName::new(scope, *name), BindingId::new(next_binding));
                next_binding = next_binding.saturating_add(1);
            }
        }

        let mut function_ids = vec![None; scopes.len()];
        let mut next_function = 0u32;
        for (scope, lexical_scope) in scopes.iter().enumerate() {
            if matches!(lexical_scope.kind, ScopeKind::Program | ScopeKind::Function) {
                function_ids[scope] = Some(FunctionId::new(next_function));
                next_function = next_function.saturating_add(1);
            }
        }

        (binding_ids, function_ids)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn empty() -> Self {
        Self {
            assignments: FrozenAssignmentIndex::from_assignments(Vec::new()),
            binding_ids: HashMap::new(),
            function_ids: Vec::new(),
            function_bindings: HashMap::new(),
            function_aliases: HashMap::new(),
            parameter_aliases: HashMap::new(),
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
