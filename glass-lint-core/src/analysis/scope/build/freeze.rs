use crate::analysis::scope::{
    ScopeGraph,
    binding_index::{BindingIndex, BindingIndexInput},
    build::{
        FrozenScopeCollectionArtifacts, ScopeCollector,
        program::{ScopeCollectionIssue, ScopedProgram},
    },
    graph::ScopeGraphInput,
    mutation_index::MutationIndex,
};

impl ScopeCollector<'_> {
    pub(crate) fn freeze(mut self, environment: &crate::Environment) -> ScopedProgram {
        if !self.scope_shapes.is_consumed() {
            self.artifacts
                .record_issue(ScopeCollectionIssue::UnconsumedShape);
        }
        let FrozenScopeCollectionArtifacts {
            scope_issues: mut issues,
            mutable_static_objects,
            property_assignments: property_artifacts,
        } = std::mem::take(&mut self.artifacts).seal();
        let parameter_aliases = self.parameter_aliases();
        let function_bindings = self
            .function_scopes
            .into_iter()
            .map(|(binding, function)| (binding, function.scope))
            .collect();
        let (binding_ids, function_ids) = BindingIndex::allocate_ids(&self.scopes);
        let bindings = BindingIndex::try_from(BindingIndexInput {
            assignments: std::mem::take(&mut self.assignments),
            binding_ids,
            function_ids,
            function_bindings,
            function_aliases: self.function_aliases,
            parameter_aliases,
        })
        .unwrap_or_else(|_| {
            issues.push(ScopeCollectionIssue::InvalidBindingIndex);
            BindingIndex::empty()
        });
        let scope_shape_valid = issues.is_empty();
        let mutations = MutationIndex::from(mutable_static_objects);
        let mut graph = ScopeGraph::from_collected(ScopeGraphInput {
            environment: environment.clone(),
            names: self.names,
            scopes: self.scopes,
            bindings,
            mutations,
            scope_shape_valid,
        });
        graph.finish_collected_properties(property_artifacts);
        let frozen = graph.freeze();
        ScopedProgram {
            graph: frozen,
            issues,
        }
    }
}
