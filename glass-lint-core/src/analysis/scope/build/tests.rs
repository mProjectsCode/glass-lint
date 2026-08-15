use swc_common::{Span, Spanned};
use swc_ecma_ast::VarDeclKind;
use swc_ecma_visit::VisitWith;

use super::*;
use crate::analysis::scope::{
    ScopeKind, ScopeTraversal,
    build::traversal::{ScopeEntry, ScopePass},
};

fn with_collector<R>(
    source: &str,
    callback: impl for<'a> FnOnce(&mut ScopeCollector<'a>) -> R,
) -> R {
    with_test_budget(|budget| {
        let parsed =
            crate::parse_test_source(source, "scope-collector.js").expect("source should parse");
        let names = glass_lint_datastructures::NameTable::default();
        let planner = plan::ScopePlanner::new(parsed.program.span(), names, budget);
        let mut plan_traversal = ScopeTraversal::new(planner);
        parsed.program.visit_children_with(&mut plan_traversal);
        let plan = plan_traversal.into_pass().finish();
        let predeclared = plan.scope_shapes.shapes_len();
        let collector = ScopeCollector::from_plan(plan, budget);
        let mut collect_traversal = ScopeTraversal::new(collector);
        parsed.program.visit_children_with(&mut collect_traversal);
        let mut collector = collect_traversal.into_pass();
        assert!(
            !collector.artifacts.has_issues(),
            "main visitor did not diverge from predeclared scopes"
        );
        assert_eq!(
            collector.scope_lookups, predeclared,
            "main visitor consumed one shape per predeclared scope",
        );
        callback(&mut collector)
    })
}
fn scope_fingerprint(collector: &ScopeCollector) -> Vec<String> {
    collector
        .lexical
        .scopes
        .iter()
        .map(|scope| {
            let mut bindings: Vec<_> = scope.binding_entries().collect();
            bindings.sort_by_key(|(id, _)| *id);
            let binding_str = bindings
                .iter()
                .map(|(id, prov)| format!("{id:?}: {prov:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "parent={:?} depth={} kind={:?} span=({}, {}) bindings={{{}}}",
                scope.parent(),
                scope.depth(),
                scope.kind(),
                scope.span().lo.0,
                scope.span().hi.0,
                binding_str,
            )
        })
        .collect()
}

fn with_planned_scopes<R>(
    span: Span,
    kinds: &[ScopeKind],
    callback: impl for<'a> FnOnce(&mut ScopeCollector<'a>) -> R,
) -> R {
    with_test_budget(|budget| {
        let names = glass_lint_datastructures::NameTable::default();
        let mut planner = plan::ScopePlanner::new(span, names, budget);
        for &kind in kinds {
            planner.push_scope(span, kind);
            planner.pop_scope();
        }
        let mut collector = ScopeCollector::from_plan(planner.finish(), budget);
        callback(&mut collector)
    })
}

fn with_planned_scopes_consuming<R>(
    span: Span,
    kinds: &[ScopeKind],
    callback: impl for<'a> FnOnce(ScopeCollector<'a>) -> R,
) -> R {
    with_test_budget(|budget| {
        let names = glass_lint_datastructures::NameTable::default();
        let mut planner = plan::ScopePlanner::new(span, names, budget);
        for &kind in kinds {
            planner.push_scope(span, kind);
            planner.pop_scope();
        }
        callback(ScopeCollector::from_plan(planner.finish(), budget))
    })
}

#[test]
fn popping_the_program_scope_invalidates_collection_instead_of_falling_back() {
    let parsed = crate::parse_test_source("const value = 1;", "scope-collector.js")
        .expect("source should parse");
    with_planned_scopes(parsed.program.span(), &[], |collector| {
        assert!(collector.current_scope().is_some());
        let program = collector.current_scope().expect("program scope");
        collector.pop_scope(ScopeEntry::Entered(program));

        assert!(collector.artifacts.has_issues());
        assert!(collector.current_scope().is_none());
        assert!(collector.binding_scope(VarDeclKind::Var).is_none());
    });
}

#[test]
fn preserves_scope_order_for_all_scope_constructs() {
    let source = r"
        function outer(parameter) {
            { let block = parameter; }
            for (let index = 0; index < 1; index++) {
                (() => { let nested = index; })();
            }
            for (const item of items) { function loopFunction() {} }
            for (const key in object) { key; }
            switch (parameter) {
                case 0: { let caseValue = parameter; break; }
                default: break;
            }
            try { throw parameter; }
            catch (error) { const caught = error; }
            with (context) { value; }
            const functionValue = function named(value) { return value; };
            const arrow = value => { return value; };
        }
    ";
    with_collector(source, |first| {
        let second = with_collector(source, |second| scope_fingerprint(second));

        assert_eq!(scope_fingerprint(first), second);
        assert!(
            first
                .lexical
                .scopes
                .iter()
                .any(|scope| scope.kind() == ScopeKind::Function)
        );
        assert!(
            first
                .lexical
                .scopes
                .iter()
                .any(|scope| scope.kind() == ScopeKind::Block)
        );
        assert!(
            first
                .lexical
                .scopes
                .iter()
                .any(|scope| scope.kind() == ScopeKind::Dynamic)
        );
        assert!(
            first
                .lexical
                .scopes
                .iter()
                .any(|scope| scope.kind() == ScopeKind::Function && scope.depth() > 2)
        );
    });
}

#[test]
fn reuses_same_span_same_kind_siblings_by_order() {
    let parsed = crate::parse_test_source("value;", "same-span.js").expect("source should parse");
    let span = parsed.program.span();
    with_planned_scopes(span, &[ScopeKind::Block, ScopeKind::Block], |collector| {
        let predeclared = collector.lexical.scope_shapes.shapes_len();
        assert_eq!(predeclared, 2);

        let entry = collector.push_scope(span, ScopeKind::Block);
        let first = collector.current_scope();
        collector.pop_scope(entry);
        collector.push_scope(span, ScopeKind::Block);
        let second = collector.current_scope();

        assert_eq!(
            (first, second),
            (Some(ScopeId::from_test(1)), Some(ScopeId::from_test(2)),)
        );
        assert_eq!(collector.scope_lookups, 2);
        assert_eq!(
            collector.lexical.scope_shapes.remaining(
                Some(ScopeId::from_test(0)),
                span.lo,
                ScopeKind::Block
            ),
            0,
        );
    });
}

fn sibling_scope_lookups(count: usize) -> usize {
    let source = (0..count)
        .map(|index| format!("{{ let value{index} = {index}; }}"))
        .collect::<Vec<_>>()
        .join("\n");
    with_collector(&source, |collector| collector.scope_lookups)
}

#[test]
fn many_sibling_scopes_consume_one_shape_each() {
    let one = sibling_scope_lookups(128);
    let two = sibling_scope_lookups(256);

    assert_eq!(one, 128);
    assert_eq!(two, one * 2);
}

#[test]
fn divergence_on_extra_scope_fails_closed() {
    let parsed =
        crate::parse_test_source("value;", "divergence-extra.js").expect("source should parse");
    let span = parsed.program.span();
    with_planned_scopes(span, &[ScopeKind::Block], |collector| {
        assert_eq!(collector.lexical.scope_shapes.shapes_len(), 1);
        let before = collector.current_scope();
        let entry = collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope(entry);
        assert!(!collector.artifacts.has_issues());
        assert_eq!(collector.current_scope(), before);
        collector.push_scope(span, ScopeKind::Block);
        assert!(collector.artifacts.has_issues());
        // The invalid collector no longer exposes a fallback scope.
        assert!(collector.current_scope().is_none());
    });
}

#[test]
fn frozen_scope_queries_fail_closed_after_shape_mismatch() {
    let parsed =
        crate::parse_test_source("value;", "frozen-divergence.js").expect("source should parse");
    let span = parsed.program.span();
    with_planned_scopes_consuming(span, &[ScopeKind::Block], |mut collector| {
        let entered = collector.push_scope(span, ScopeKind::Block);
        assert!(matches!(entered, ScopeEntry::Entered(_)));
        collector.pop_scope(entered);
        let rejected = collector.push_scope(span, ScopeKind::Block);
        assert!(matches!(rejected, ScopeEntry::Rejected));

        let scoped = collector.freeze(&crate::Environment::default());
        assert!(scoped.issues.contains(&ScopeCollectionIssue::ShapeMismatch));
        assert_eq!(scoped.graph.scope_at(span), None);
    });
}

#[test]
fn divergence_on_missing_scope_fails_closed() {
    let parsed =
        crate::parse_test_source("value;", "divergence-missing.js").expect("source should parse");
    let span = parsed.program.span();
    with_planned_scopes(span, &[ScopeKind::Block, ScopeKind::Block], |collector| {
        assert_eq!(collector.lexical.scope_shapes.shapes_len(), 2);
        let entry = collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope(entry);
        assert!(!collector.artifacts.has_issues());
        assert_eq!(
            collector.lexical.scope_shapes.remaining(
                Some(ScopeId::from_test(0)),
                span.lo,
                ScopeKind::Block
            ),
            1,
            "the unvisited predeclared shape stays in the table",
        );
        // A second visit consumes the remaining predeclared shape.
        collector.push_scope(span, ScopeKind::Block);
        assert!(!collector.artifacts.has_issues());
        // A third visit finds no matching shape and fails closed.
        collector.push_scope(span, ScopeKind::Block);
        assert!(collector.artifacts.has_issues());
        // The invalid collector no longer exposes a fallback scope.
        assert!(collector.current_scope().is_none());
    });
}

#[test]
fn divergence_on_kind_mismatch_fails_closed() {
    let parsed =
        crate::parse_test_source("value;", "divergence-kind.js").expect("source should parse");
    let span = parsed.program.span();
    with_planned_scopes(span, &[ScopeKind::Block], |collector| {
        collector.push_scope(span, ScopeKind::Function);
        assert!(collector.artifacts.has_issues());
        // The invalid collector no longer exposes a fallback scope.
        assert!(collector.current_scope().is_none());
    });
}

#[test]
fn hoisted_var_in_blocks_preserves_function_scoping() {
    let source = r"
        function outer() {
            if (true) { var hoisted = 1; }
            return hoisted;
        }
    ";
    with_collector(source, |collector| {
        let function_scopes: Vec<_> = collector
            .lexical
            .scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.kind() == ScopeKind::Function)
            .collect();
        assert_eq!(function_scopes.len(), 1);
        let (fn_idx, fn_scope) = function_scopes[0];
        assert!(
            fn_scope.has_bindings(),
            "function scope {fn_idx} has no bindings",
        );

        let block_scopes: Vec<_> = collector
            .lexical
            .scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.kind() == ScopeKind::Block)
            .collect();
        // var hoisted into function scope means block scopes should not have
        // the hoisted binding
        for (idx, scope) in &block_scopes {
            let is_empty = !scope
                .binding_entries()
                .any(|(_, p)| matches!(p, BindingProvenance::Local));
            assert!(is_empty, "block scope {idx} contains var bindings");
        }
    });
}

#[test]
fn catch_without_param_forms_valid_scope() {
    let source = r"
        try { let a = 1; } catch { let b = 2; }
    ";
    with_collector(source, |first| {
        let second = with_collector(source, |second| scope_fingerprint(second));
        assert_eq!(scope_fingerprint(first), second);
        assert!(
            first
                .lexical
                .scopes
                .iter()
                .any(|scope| scope.kind() == ScopeKind::Block && scope.depth() == 1)
        );
    });
}

#[test]
fn loops_with_and_without_inits_form_valid_scopes() {
    let source = r"
        for (;;) { break; }
        for (let i = 0; i < 1; i++) { break; }
        for (const x of []) { break; }
        for (const k in {}) { break; }
    ";
    with_collector(source, |first| {
        let (second_fingerprint, second_blocks) = with_collector(source, |second| {
            (
                scope_fingerprint(second),
                second
                    .lexical
                    .scopes
                    .iter()
                    .filter(|scope| scope.kind() == ScopeKind::Block)
                    .count(),
            )
        });
        assert_eq!(scope_fingerprint(first), second_fingerprint);
        assert_eq!(
            first
                .lexical
                .scopes
                .iter()
                .filter(|scope| scope.kind() == ScopeKind::Block)
                .count(),
            second_blocks
        );
    });
}

#[test]
fn with_statement_creates_dynamic_scope() {
    let source = r"
        const obj = {};
        with (obj) { let value = prop; }
    ";
    with_collector(source, |first| {
        let second = with_collector(source, |second| scope_fingerprint(second));
        assert_eq!(scope_fingerprint(first), second);
        assert!(
            first
                .lexical
                .scopes
                .iter()
                .any(|scope| scope.kind() == ScopeKind::Dynamic)
        );
    });
}

#[test]
fn switch_with_cases_forms_block_scope() {
    let source = r"
        switch (a) { case 0: { let b = 1; break; } default: break; }
    ";
    with_collector(source, |first| {
        let second = with_collector(source, |second| scope_fingerprint(second));
        assert_eq!(scope_fingerprint(first), second);
        // Switch body is a block scope
        assert!(
            first
                .lexical
                .scopes
                .iter()
                .any(|scope| scope.kind() == ScopeKind::Block && scope.depth() == 1)
        );
    });
}

#[test]
fn nested_function_and_arrow_scopes_have_correct_depths() {
    let source = r"
        function a() {
            function b() {
                const c = () => { return 1; };
                c();
            }
            b();
        }
    ";
    with_collector(source, |collector| {
        let function_depths: Vec<_> = collector
            .lexical
            .scopes
            .iter()
            .filter(|scope| scope.kind() == ScopeKind::Function)
            .map(crate::analysis::model::scope::LexicalScope::depth)
            .collect();
        // Function bodies have intervening block scopes:
        // depth 1 = a, depth 3 = b (after a-block), depth 5 = arrow c (after a-block +
        // b-block)
        assert!(function_depths.contains(&1));
        assert!(function_depths.contains(&3));
        assert!(function_depths.contains(&5));
    });
}

mod tests_extended;
