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
    let parsed = crate::parse_test_source(
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
fn redeclaration_resets_the_planned_binding_provenance() {
    let parsed = crate::parse_test_source(
        "var value = 'known'; var value = left + right; use(value);",
        "redeclared-binding.js",
    )
    .expect("source should parse");
    let graph = ScopeGraph::collect(&parsed.program);
    let mut collector = IdentCollector::default();
    parsed.program.visit_with(&mut collector);
    collector.values.sort_by_key(|ident| ident.span.lo);

    assert!(matches!(
        graph.preferred_binding_witness_at("value", collector.values[2].span),
        Some(BindingProvenance::Local)
    ));
}

#[test]
fn repeated_scope_queries_preserve_nested_and_cross_scope_results() {
    let parsed = crate::parse_test_source(
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

    let program_scope = graph.scope_at(program_uses[0].span).expect("program scope");
    let block_scope = graph.scope_at(program_uses[1].span).expect("block scope");
    let function_scope = graph.scope_at(block_use.span).expect("function scope");
    assert_eq!(graph.scope_at(block_use.span), Some(function_scope));
    assert_eq!(graph.scope_kind(program_scope), Some(ScopeKind::Program));
    assert_eq!(graph.scope_parent(block_scope), Some(program_scope));
    assert_ne!(function_scope, block_scope);

    let cross_scope_span = swc_common::Span::new(block_use.span.lo, program_uses[2].span.hi);
    assert_eq!(graph.scope_at(cross_scope_span), Some(program_scope));
    assert_eq!(graph.scope_at(cross_scope_span), Some(program_scope));
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

    let parsed = crate::parse_test_source(
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
            .preferred_binding_witness_at("PluginSettingTab", identifiers[1].span)
            .is_some()
    );
}
