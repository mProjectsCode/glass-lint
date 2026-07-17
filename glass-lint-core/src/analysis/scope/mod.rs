//! Lexical scopes plus the narrow alias facts needed by semantic matching.
//!
//! This is not a general JavaScript interpreter. It records only stable facts
//! that can be followed without speculation: imports, unshadowed globals,
//! direct aliases, selected static shapes, and prior assignments. Unknown or
//! mutable cases intentionally resolve to local/absent provenance.
//!
//! Collection is split into declaration prepass, source-order provenance, and
//! immutable query indexes. Binding IDs and assignment versions make later
//! queries position-sensitive without rebuilding the AST.

use collect::LexicalScopeCollector;
use swc_common::Spanned;
use swc_ecma_ast::Program;
use swc_ecma_visit::VisitWith;

mod collect;
mod query;

mod model;
pub(in crate::analysis) use model::*;

impl ScopeGraph {
    #[cfg(test)]
    pub(super) fn collect(program: &Program) -> Self {
        Self::collect_with_environment(program, &crate::Environment::default())
    }

    /// Build one matcher-independent scope graph using the configured globals.
    pub(super) fn collect_with_environment(
        program: &Program,
        environment: &crate::Environment,
    ) -> Self {
        let mut collector = LexicalScopeCollector::new(program.span());
        // Build declarations before collecting initializers and uses.  This
        // makes the resolver position-aware without making it traversal-order
        // dependent: an earlier use of a later declaration is local/TDZ, not
        // an accidentally unshadowed global.
        collector.predeclare(program);
        program.visit_children_with(&mut collector);
        collector.freeze(environment)
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
