use glass_lint_datastructures::NameId;
use hashbrown::HashMap;
use swc_common::{BytePos, Span};

use crate::analysis::{
    model::scope::{
        AliasAssignment, BindingId, BindingProvenance, BindingVersion, FunctionId, LexicalScopes,
        ScopeId, ScopeKind, ScopedName,
    },
    scope::frozen_assignments::{AssignmentAt, FrozenAssignmentIndex},
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

/// Stable IDs allocated from the collector's lexical scopes.
#[derive(Debug)]
pub(super) struct BindingAllocation {
    pub(super) binding_ids: HashMap<ScopedName, BindingId>,
    pub(super) function_ids: HashMap<ScopeId, FunctionId>,
    pub(super) function_spans: HashMap<FunctionId, Span>,
}

/// Collector-side facts consumed by the binding-index freeze transition.
#[derive(Debug)]
pub(super) struct BindingFreezeInput {
    pub(super) assignments: Vec<AliasAssignment>,
    pub(super) allocation: BindingAllocation,
    pub(super) function_bindings: HashMap<ScopedName, ScopeId>,
    pub(super) function_aliases: HashMap<ScopedName, ScopeId>,
    pub(super) parameter_aliases: HashMap<ScopedName, BindingProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BindingIndexError {
    pub(super) scope: ScopeId,
}

#[derive(Debug)]
pub(super) struct BindingIndex {
    assignments: FrozenAssignmentIndex,
    binding_ids: HashMap<ScopedName, BindingId>,
    function_ids: HashMap<ScopeId, FunctionId>,
    function_spans: HashMap<FunctionId, Span>,
    function_bindings: HashMap<ScopedName, FunctionId>,
    function_aliases: HashMap<ScopedName, FunctionId>,
    parameter_aliases: HashMap<ParameterAliasKey, BindingProvenance>,
}

impl BindingIndex {
    pub(super) fn from_freeze_input(input: BindingFreezeInput) -> Result<Self, BindingIndexError> {
        let BindingFreezeInput {
            assignments,
            allocation:
                BindingAllocation {
                    binding_ids,
                    function_ids,
                    function_spans,
                },
            function_bindings,
            function_aliases,
            parameter_aliases,
        } = input;
        let function_bindings = resolve_function_targets(function_bindings, &function_ids)?;
        let function_aliases = resolve_function_targets(function_aliases, &function_ids)?;
        let parameter_aliases = resolve_parameter_aliases(parameter_aliases, &function_ids)?;
        Ok(Self {
            assignments: FrozenAssignmentIndex::from_assignments(assignments),
            binding_ids,
            function_ids,
            function_spans,
            function_bindings,
            function_aliases,
            parameter_aliases,
        })
    }
}

fn resolve_function_targets(
    entries: HashMap<ScopedName, ScopeId>,
    function_ids: &HashMap<ScopeId, FunctionId>,
) -> Result<HashMap<ScopedName, FunctionId>, BindingIndexError> {
    entries
        .into_iter()
        .map(|(name, scope)| {
            function_for_scope(function_ids, scope).map(|function| (name, function))
        })
        .collect()
}

fn resolve_parameter_aliases(
    entries: HashMap<ScopedName, BindingProvenance>,
    function_ids: &HashMap<ScopeId, FunctionId>,
) -> Result<HashMap<ParameterAliasKey, BindingProvenance>, BindingIndexError> {
    entries
        .into_iter()
        .map(|(name, provenance)| {
            function_for_scope(function_ids, name.scope())
                .map(|function| (ParameterAliasKey::new(function, name.name()), provenance))
        })
        .collect()
}

fn function_for_scope(
    function_ids: &HashMap<ScopeId, FunctionId>,
    scope: ScopeId,
) -> Result<FunctionId, BindingIndexError> {
    function_ids
        .get(&scope)
        .copied()
        .ok_or(BindingIndexError { scope })
}

impl BindingIndex {
    /// Allocate stable binding and function IDs over the lexical scopes.
    pub(super) fn allocate_ids(scopes: &LexicalScopes) -> BindingAllocation {
        let mut binding_ids = HashMap::new();
        let mut next_binding = 0u32;
        for scope in scopes.ids() {
            let Some(lexical_scope) = scopes.get(scope) else {
                continue;
            };
            for name in lexical_scope.binding_names() {
                binding_ids.insert(ScopedName::new(scope, *name), BindingId::new(next_binding));
                next_binding = next_binding.saturating_add(1);
            }
        }

        let mut function_ids = HashMap::new();
        let mut function_spans = HashMap::new();
        let mut next_function = 0u32;
        for scope in scopes.ids() {
            let Some(lexical_scope) = scopes.get(scope) else {
                continue;
            };
            if matches!(
                lexical_scope.kind(),
                ScopeKind::Program | ScopeKind::Function
            ) {
                let function = FunctionId::new(next_function);
                function_ids.insert(scope, function);
                function_spans.insert(function, lexical_scope.span());
                next_function = next_function.saturating_add(1);
            }
        }

        BindingAllocation {
            binding_ids,
            function_ids,
            function_spans,
        }
    }

    pub(super) fn empty() -> Self {
        Self {
            assignments: FrozenAssignmentIndex::from_assignments(Vec::new()),
            binding_ids: HashMap::new(),
            function_ids: HashMap::new(),
            function_spans: HashMap::new(),
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
        self.function_ids.get(&scope).copied()
    }

    pub(super) fn function_span(&self, function: FunctionId) -> Option<Span> {
        self.function_spans.get(&function).copied()
    }

    pub(super) fn function_containing(&self, span: Span) -> Option<FunctionId> {
        self.function_spans
            .iter()
            .filter_map(|(function, candidate)| {
                (candidate.lo <= span.lo && candidate.hi >= span.hi)
                    .then_some((candidate.hi.0 - candidate.lo.0, *function))
            })
            .min()
            .map(|(_, function)| function)
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
