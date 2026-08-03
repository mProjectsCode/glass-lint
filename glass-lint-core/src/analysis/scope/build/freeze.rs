use crate::analysis::scope::{
    ScopeGraph,
    binding_index::{BindingIndex, BindingIndexInput},
    build::{
        ScopeCollector,
        program::{ScopeCollectionIssue, ScopedProgram},
    },
    mutation_index::MutationIndex,
};

impl ScopeCollector<'_> {
    pub(crate) fn freeze(mut self, environment: &crate::Environment) -> ScopedProgram {
        if !self.scope_shapes.is_consumed() {
            self.artifacts
                .record_issue(ScopeCollectionIssue::UnconsumedShape);
        }
        let scope_shape_valid = !self.artifacts.has_issues();
        let (issues, mutable_static_objects, property_artifacts) =
            std::mem::take(&mut self.artifacts)
                .finish_into()
                .into_parts();
        let parameter_aliases = self.parameter_aliases();
        let function_bindings = self
            .function_scopes
            .into_iter()
            .map(|(binding, function)| (binding, function.scope))
            .collect();
        let (binding_ids, function_ids) = BindingIndex::allocate_ids(&self.scopes);
        let bindings = BindingIndex::from(BindingIndexInput {
            assignments: std::mem::take(&mut self.assignments),
            binding_ids,
            function_ids,
            function_bindings,
            function_aliases: self.function_aliases,
            parameter_aliases,
        });
        let mutations = MutationIndex::from(mutable_static_objects);
        let mut graph = ScopeGraph::from_collected(
            environment.clone(),
            self.names,
            self.scopes,
            bindings,
            mutations,
            scope_shape_valid,
        );
        graph.finish_collected_properties(property_artifacts);
        let frozen = graph.freeze();
        ScopedProgram {
            graph: frozen,
            issues,
        }
    }
}
