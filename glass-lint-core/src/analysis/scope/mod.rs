//! Lexical scopes plus the narrow alias facts needed by semantic matching.
//!
//! This is not a general JavaScript interpreter. It records only stable facts
//! that can be followed without speculation: imports, unshadowed globals,
//! direct aliases, selected static shapes, and prior assignments. Unknown or
//! mutable cases intentionally resolve to local/absent provenance.

use std::collections::BTreeMap;

use swc_common::Spanned;
use swc_ecma_ast::Program;
use swc_ecma_visit::VisitWith;

use super::value::{BindingId, FunctionId};
use collect::{AliasCollector, PropertyAliasAssignment, RootedPropertyMutation};

mod collect;
mod query;

mod model;
pub(in crate::analysis) use model::*;

impl ScopeGraph {
    #[cfg(test)]
    pub(super) fn collect(program: &Program) -> Self {
        Self::collect_with_environment(program, &crate::Environment::default())
    }

    pub(super) fn collect_with_environment(
        program: &Program,
        environment: &crate::Environment,
    ) -> Self {
        let mut collector = AliasCollector::new(program.span());
        // Build declarations before collecting initializers and uses.  This
        // makes the resolver position-aware without making it traversal-order
        // dependent: an earlier use of a later declaration is local/TDZ, not
        // an accidentally unshadowed global.
        collector.predeclare(program);
        program.visit_children_with(&mut collector);
        let parameter_aliases_by_scope = collector.parameter_aliases();
        // Scope lookup starts from the latest opening delimiter, then walks to
        // parents only when the candidate does not contain the queried span.
        let mut scopes_by_start = (0..collector.scopes.len()).collect::<Vec<_>>();
        scopes_by_start.sort_by_key(|index| {
            let scope = &collector.scopes[*index];
            (scope.span.lo, scope.depth)
        });
        let mut assignments = BTreeMap::<usize, BTreeMap<String, Vec<AliasAssignment>>>::new();
        for assignment in collector.assignments {
            assignments
                .entry(assignment.scope)
                .or_default()
                .entry(assignment.name.clone())
                .or_default()
                .push(assignment);
        }
        for scope_assignments in assignments.values_mut() {
            for binding_assignments in scope_assignments.values_mut() {
                binding_assignments.sort_by_key(|assignment| assignment.span.lo);
            }
        }
        let mut binding_ids = BTreeMap::new();
        let mut next_binding_id = 0u32;
        for (scope, lexical_scope) in collector.scopes.iter().enumerate() {
            for name in lexical_scope.bindings.keys() {
                binding_ids.insert((scope, name.clone()), BindingId(next_binding_id));
                next_binding_id = next_binding_id.saturating_add(1);
            }
        }
        let mut function_ids = BTreeMap::new();
        let mut next_function_id = 0u32;
        for (scope, lexical_scope) in collector.scopes.iter().enumerate() {
            if matches!(lexical_scope.kind, ScopeKind::Program | ScopeKind::Function) {
                function_ids.insert(scope, FunctionId(next_function_id));
                next_function_id = next_function_id.saturating_add(1);
            }
        }
        let function_bindings = collector
            .function_scopes
            .iter()
            .filter_map(|((scope, name), (function_scope, _))| {
                function_ids
                    .get(function_scope)
                    .copied()
                    .map(|function| ((*scope, name.clone()), function))
            })
            .collect();
        let function_aliases = collector
            .function_aliases
            .into_iter()
            .filter_map(|((scope, name), function_scope)| {
                function_ids
                    .get(&function_scope)
                    .copied()
                    .map(|function| ((scope, name), function))
            })
            .collect();
        let parameter_aliases = parameter_aliases_by_scope
            .into_iter()
            .filter_map(|((scope, name), provenance)| {
                function_ids
                    .get(&scope)
                    .copied()
                    .map(|function| ((function, name), provenance))
            })
            .collect();
        let collected_property_assignments = collector.property_assignments;
        let collected_rooted_property_mutations = collector.rooted_property_mutations;
        let mut graph = Self {
            environment: environment.clone(),
            scopes: collector.scopes,
            scopes_by_start,
            assignments,
            binding_ids,
            function_ids,
            function_bindings,
            function_aliases,
            property_assignments: BTreeMap::new(),
            rooted_property_mutations: BTreeMap::new(),
            parameter_aliases,
            dynamic_evals: Vec::new(),
            mutable_static_objects: collector.mutable_static_objects.clone(),
        };
        graph.finish_collected_properties(
            collected_property_assignments,
            collected_rooted_property_mutations,
            collector.dynamic_evals,
        );
        graph
    }

    fn finish_collected_properties(
        &mut self,
        property_assignments: Vec<PropertyAliasAssignment>,
        rooted_mutations: Vec<RootedPropertyMutation>,
        dynamic_evals: Vec<(usize, swc_common::Span)>,
    ) {
        for assignment in property_assignments {
            let Some(receiver_key) = self
                .binding_key_for_name(assignment.receiver.sym.as_ref(), assignment.receiver.span)
            else {
                continue;
            };
            self.property_assignments
                .entry((
                    receiver_key,
                    assignment
                        .property
                        .strip_prefix(assignment.receiver.sym.as_ref())
                        .and_then(|path| path.strip_prefix('.'))
                        .map(|path| path.split('.').map(str::to_string).collect::<Vec<_>>())
                        .unwrap_or_default(),
                ))
                .or_default()
                .push(PropertyAliasFact {
                    span: assignment.span,
                    scope: assignment.scope,
                    target: assignment.target.map(std::convert::Into::into),
                });
        }
        for assignments in self.property_assignments.values_mut() {
            assignments.sort_by_key(|assignment| assignment.span.lo);
        }
        for mutation in rooted_mutations {
            self.rooted_property_mutations
                .entry(mutation.receiver)
                .or_default()
                .push(RootedPropertyMutationFact {
                    span: mutation.span,
                    scope: mutation.scope,
                    property: mutation.property,
                });
        }
        for mutations in self.rooted_property_mutations.values_mut() {
            mutations.sort_by_key(|mutation| mutation.span.lo);
        }
        self.dynamic_evals = dynamic_evals
            .into_iter()
            .filter(|(_, span)| self.binding_at("eval", *span).is_none())
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_ecma_ast::{Expr, Ident};
    use swc_ecma_visit::{Visit, VisitWith};

    #[derive(Default)]
    struct IdentCollector {
        values: Vec<Ident>,
    }

    impl Visit for IdentCollector {
        fn visit_ident(&mut self, ident: &Ident) {
            if ident.sym == *"value" {
                self.values.push(ident.clone());
            }
        }
    }

    #[test]
    fn binding_keys_change_at_assignment_versions() {
        let parsed = crate::parse(
            "let value = source; value = replacement; use(value);",
            "bindings.js",
        )
        .expect("source should parse");
        let graph = ScopeGraph::collect(&parsed.program);
        let mut collector = IdentCollector::default();
        parsed.program.visit_with(&mut collector);
        collector.values.sort_by_key(|ident| ident.span.lo);
        let keys = collector
            .values
            .iter()
            .map(|ident| graph.binding_key_for_expr(&Expr::Ident(ident.clone())))
            .collect::<Vec<_>>();
        assert!(keys.iter().all(Option::is_some));
        assert_ne!(keys[0], keys[1]);
        assert_eq!(keys[1], keys[2]);
    }
}
