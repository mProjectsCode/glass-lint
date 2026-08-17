use crate::analysis::scope::{
    ScopeGraph,
    binding_index::{BindingFreezeInput, BindingIndex},
    build::{
        ScopeCollectionArtifacts, ScopeCollector,
        program::{ScopeCollectionIssue, ScopedProgram},
    },
    mutation_index::MutationIndexBuilder,
};

impl ScopeCollector<'_> {
    pub(crate) fn freeze(mut self, environment: &crate::Environment) -> ScopedProgram {
        if !self.lexical.scope_shapes.is_consumed() {
            self.artifacts
                .record_issue(ScopeCollectionIssue::UnconsumedShape);
        }
        let parameter_aliases = self.parameter_aliases();
        let ScopeCollectionArtifacts {
            scope_issues: mut issues,
            mutable_static_objects,
            property_assignments,
            rooted_property_mutations,
            dynamic_evals,
        } = self.artifacts;
        let function_bindings = self
            .functions
            .function_scopes
            .into_iter()
            .map(|(binding, function)| (binding, function.scope))
            .collect();
        let (binding_ids, function_ids, function_spans) =
            BindingIndex::allocate_ids(&self.lexical.scopes);
        let bindings = BindingIndex::from_freeze_input(BindingFreezeInput {
            assignments: std::mem::take(&mut self.assignment.assignments),
            binding_ids,
            function_ids,
            function_spans,
            function_bindings,
            function_aliases: self.functions.function_aliases,
            parameter_aliases,
        })
        .unwrap_or_else(|_| {
            issues.push(ScopeCollectionIssue::InvalidBindingIndex);
            BindingIndex::empty()
        });
        let scope_shape_valid = issues.is_empty();
        let mutations = MutationIndexBuilder::from(mutable_static_objects);
        let mut graph = ScopeGraph::from_collected(
            environment.clone(),
            self.lexical.names,
            self.lexical.scopes,
            bindings,
            mutations,
            scope_shape_valid,
        );
        graph.finish_collected_properties(
            property_assignments,
            rooted_property_mutations,
            dynamic_evals,
        );
        let frozen = graph.freeze();
        ScopedProgram {
            graph: frozen,
            issues,
        }
    }
}
