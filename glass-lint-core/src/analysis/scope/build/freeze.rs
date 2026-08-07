use crate::analysis::scope::{
    ScopeGraph,
    binding_index::{BindingIndex, BindingIndexInput},
    build::{
        FrozenScopeCollectionArtifacts, ScopeCollector,
        program::{ScopeCollectionIssue, ScopedProgram},
    },
    graph::ScopeGraphInput,
    mutation_index::MutationIndexBuilder,
};

impl ScopeCollector<'_> {
    pub(crate) fn freeze(mut self, environment: &crate::Environment) -> ScopedProgram {
        if !self.lexical.scope_shapes.is_consumed() {
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
            .functions
            .function_scopes
            .into_iter()
            .map(|(binding, function)| (binding, function.scope))
            .collect();
        let (binding_ids, function_ids, function_spans) =
            BindingIndex::allocate_ids(&self.lexical.scopes);
        let bindings = BindingIndex::try_from(BindingIndexInput {
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
        let mut graph = ScopeGraph::from_collected(ScopeGraphInput {
            environment: environment.clone(),
            names: self.lexical.names,
            scopes: self.lexical.scopes,
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
