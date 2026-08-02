use hashbrown::HashMap;

use crate::analysis::{
    scope::{
        FrozenAssignmentIndex, LexicalScope, ScopeGraph, ScopeGraphParts, ScopeId, ScopedName,
        binding_index::{BindingIndexParts, ParameterAliasKey},
        build::{
            ScopeCollector,
            program::{ScopeCollectionIssue, ScopedProgram},
        },
        graph::LexicalScopeParts,
        mutation_index::MutationIndexParts,
    },
    value::{BindingId, FunctionId},
};

impl ScopeCollector<'_> {
    fn sorted_scope_starts(scopes: &[LexicalScope]) -> Vec<ScopeId> {
        let mut scopes_by_start: Vec<_> = (0..scopes.len()).map(ScopeId::new).collect();
        scopes_by_start.sort_by_key(|index| {
            let scope = &scopes[index.index()];
            (scope.span.lo, scope.depth)
        });
        scopes_by_start
    }

    fn allocate_ids(
        scopes: &[LexicalScope],
    ) -> (HashMap<ScopedName, BindingId>, Vec<Option<FunctionId>>) {
        let mut binding_ids = HashMap::new();
        let mut next_binding = 0u32;
        for (scope, lexical_scope) in scopes.iter().enumerate() {
            let scope = ScopeId::new(scope);
            for name in lexical_scope.bindings.keys() {
                binding_ids.insert(ScopedName::new(scope, *name), BindingId::new(next_binding));
                next_binding = next_binding.saturating_add(1);
            }
        }

        let mut function_ids = vec![None; scopes.len()];
        let mut next_function = 0u32;
        for (scope, lexical_scope) in scopes.iter().enumerate() {
            if matches!(
                lexical_scope.kind,
                crate::analysis::scope::ScopeKind::Program
                    | crate::analysis::scope::ScopeKind::Function
            ) {
                function_ids[scope] = Some(FunctionId::new(next_function));
                next_function = next_function.saturating_add(1);
            }
        }

        (binding_ids, function_ids)
    }

    pub(crate) fn freeze(mut self, environment: &crate::Environment) -> ScopedProgram {
        if !self.scope_shapes.is_consumed() {
            self.artifacts
                .scope_issues
                .push(ScopeCollectionIssue::UnconsumedShape);
        }
        let scope_shape_valid = self.artifacts.scope_issues.is_empty();
        let issues = std::mem::take(&mut self.artifacts.scope_issues);
        let parameter_aliases_by_scope = self.parameter_aliases();
        let scopes_by_start = Self::sorted_scope_starts(&self.scopes);
        let assignments =
            FrozenAssignmentIndex::from_assignments(std::mem::take(&mut self.assignments));
        let (binding_ids, function_ids) = Self::allocate_ids(&self.scopes);

        let function_bindings = self
            .function_scopes
            .iter()
            .filter_map(|(binding, function)| {
                function_ids
                    .get(function.scope.index())
                    .and_then(|&opt| opt)
                    .map(|function_id| (binding.clone(), function_id))
            })
            .collect();
        let function_aliases = self
            .function_aliases
            .into_iter()
            .filter_map(|(key, function_scope)| {
                function_ids
                    .get(function_scope.index())
                    .and_then(|&opt| opt)
                    .map(|function| (key, function))
            })
            .collect();
        let parameter_aliases = parameter_aliases_by_scope
            .into_iter()
            .filter_map(|(key, provenance)| {
                function_ids
                    .get(key.scope().index())
                    .and_then(|&opt| opt)
                    .map(|function| (ParameterAliasKey::new(function, key.name()), provenance))
            })
            .collect();

        let property_assignments = self.artifacts.property_assignments;
        let rooted_mutations = self.artifacts.rooted_property_mutations;
        let dynamic_evals = self.artifacts.dynamic_evals;
        let mut graph = ScopeGraph::from_parts(ScopeGraphParts {
            environment: environment.clone(),
            lexical: LexicalScopeParts {
                names: self.names,
                scopes: self.scopes,
                scopes_by_start,
            },
            bindings: BindingIndexParts {
                assignments,
                binding_ids,
                function_ids,
                function_bindings,
                function_aliases,
                parameter_aliases,
            },
            mutations: MutationIndexParts {
                mutable_static_objects: self.artifacts.mutable_static_objects,
            },
            scope_shape_valid,
        });
        graph.finish_collected_properties(property_assignments, rooted_mutations, dynamic_evals);
        let frozen = graph.freeze();
        ScopedProgram {
            graph: frozen,
            issues,
        }
    }
}
