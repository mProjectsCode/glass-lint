//! Lexical scopes plus the narrow alias facts needed by semantic matching.
//!
//! This is not a general JavaScript interpreter. It records only stable facts
//! that can be followed without speculation: imports, unshadowed globals,
//! direct aliases, selected static shapes, and prior assignments. Unknown or
//! mutable cases intentionally resolve to local/absent provenance.
//!
//! Collection is split into three phases:
//! 1. Declaration planning — all hoisted and block-scoped declarations are
//!    registered before any initializer is visited, so a use-before-decl
//!    resolves as local/TDZ rather than an unshadowed global.
//! 2. Source-order visitation — initializers, expressions, and nested scopes
//!    are visited in AST order.
//! 3. Freeze — the collected graph is sealed into an immutable query index.
//!
//! Binding IDs and assignment versions make position-sensitive queries
//! possible without rebuilding the AST for each lookup.

use collect::{ScopeCollector, plan::ScopePlanner, traversal::ScopeTraversal};
use glass_lint_datastructures::NameTable;
use swc_common::Spanned;
use swc_ecma_ast::Program;
use swc_ecma_visit::VisitWith;

mod collect;
mod provenance_const;
mod query;

mod model;

pub(in crate::analysis) use model::*;
use provenance_const::provenance_to_const_value;

impl ScopeGraph {
    #[cfg(test)]
    pub(super) fn collect(program: &Program) -> FrozenScopeGraph {
        let scoped = Self::collect_scoped_program(
            program,
            &crate::Environment::default(),
            NameTable::default(),
        );
        scoped.into_parts().0
    }

    pub(super) fn collect_scoped_program(
        program: &Program,
        environment: &crate::Environment,
        names: NameTable,
    ) -> collect::ScopedProgram {
        let planner = ScopePlanner::new(program.span(), names);
        let mut plan_traversal = ScopeTraversal::new(planner);
        program.visit_children_with(&mut plan_traversal);
        let plan = plan_traversal.into_pass().finish();

        let collector = ScopeCollector::from_plan(plan);
        let mut collect_traversal = ScopeTraversal::new(collector);
        program.visit_children_with(&mut collect_traversal);
        collect_traversal.into_pass().freeze(environment)
    }
}

#[cfg(test)]
mod tests {
    use swc_ecma_ast::{Expr, Ident};
    use swc_ecma_visit::{Visit, VisitWith};

    use super::*;

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

    #[derive(Default)]
    struct ScopeIdentCollector {
        values: Vec<Ident>,
    }

    impl Visit for ScopeIdentCollector {
        fn visit_ident(&mut self, ident: &Ident) {
            if matches!(ident.sym.as_ref(), "program_value" | "block_value") {
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

    #[test]
    fn repeated_scope_queries_preserve_nested_and_cross_scope_results() {
        let parsed = crate::parse(
            r"
                let program_value = 0;
                {
                    let block_value = program_value;
                    function nested() { return block_value; }
                }
                program_value;
            ",
            "nested-scopes.js",
        )
        .expect("source should parse");
        let graph = ScopeGraph::collect(&parsed.program);
        let mut collector = ScopeIdentCollector::default();
        parsed.program.visit_with(&mut collector);

        let program_uses = collector
            .values
            .iter()
            .filter(|ident| ident.sym == *"program_value")
            .collect::<Vec<_>>();
        let block_use = collector
            .values
            .iter()
            .find(|ident| ident.sym == *"block_value" && ident.span.lo > program_uses[1].span.lo)
            .expect("nested block use should exist");

        let program_scope = graph.scope_at(program_uses[0].span);
        let block_scope = graph.scope_at(program_uses[1].span);
        let function_scope = graph.scope_at(block_use.span);
        assert_eq!(graph.scope_at(block_use.span), function_scope);
        assert_eq!(graph.scope_kind(program_scope), Some(ScopeKind::Program));
        assert_eq!(graph.scope_parent(block_scope), Some(program_scope));
        assert_ne!(function_scope, block_scope);

        let cross_scope_span = swc_common::Span::new(block_use.span.lo, program_uses[2].span.hi);
        assert_eq!(graph.scope_at(cross_scope_span), program_scope);
        assert_eq!(graph.scope_at(cross_scope_span), program_scope);
    }

    #[test]
    fn function_parameters_remain_local_with_compact_scope_names() {
        struct Names<'a>(&'a mut Vec<Ident>);
        impl Visit for Names<'_> {
            fn visit_ident(&mut self, ident: &Ident) {
                if ident.sym == *"PluginSettingTab" {
                    self.0.push(ident.clone());
                }
            }
        }

        let parsed = crate::parse(
            "function shadowed(PluginSettingTab) { new PluginSettingTab(); }",
            "parameter.js",
        )
        .expect("source should parse");
        let graph = ScopeGraph::collect(&parsed.program);
        let mut identifiers = Vec::new();
        parsed.program.visit_with(&mut Names(&mut identifiers));
        identifiers.sort_by_key(|ident| ident.span.lo);
        assert_eq!(identifiers.len(), 2);
        assert!(
            graph
                .binding_at("PluginSettingTab", identifiers[1].span)
                .is_some()
        );
    }
}
