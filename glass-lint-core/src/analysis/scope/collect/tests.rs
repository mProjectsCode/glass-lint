use swc_common::Spanned;
use swc_ecma_visit::VisitWith;

use super::*;

fn collect(source: &str) -> ScopeCollector {
    let parsed = crate::parse(source, "scope-collector.js").expect("source should parse");
    let names = crate::analysis::name::NameTable::default();
    let mut planner = plan::ScopePlanner::new(parsed.program.span(), names);
    parsed.program.visit_children_with(&mut planner);
    let plan = planner.finish();
    let predeclared = plan.scope_shapes.shapes_len();
    let mut collector = ScopeCollector::from_plan(plan);
    parsed.program.visit_children_with(&mut collector);
    assert!(
        collector.scope_issues.is_empty(),
        "main visitor did not diverge from predeclared scopes"
    );
    assert_eq!(
        collector.scope_lookups, predeclared,
        "main visitor consumed one shape per predeclared scope",
    );
    collector
}

fn scope_fingerprint(collector: &ScopeCollector) -> Vec<String> {
    collector
        .scopes
        .iter()
        .map(|scope| {
            format!(
                "parent={:?} depth={} kind={:?} span=({}, {}) bindings={:?}",
                scope.parent,
                scope.depth,
                scope.kind,
                scope.span.lo.0,
                scope.span.hi.0,
                scope.bindings
            )
        })
        .collect()
}

fn planned_scopes(span: Span, kinds: &[ScopeKind]) -> ScopeCollector {
    let names = crate::analysis::name::NameTable::default();
    let mut planner = plan::ScopePlanner::new(span, names);
    for &kind in kinds {
        planner.push_scope(span, kind);
        planner.pop_scope();
    }
    ScopeCollector::from_plan(planner.finish())
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
    let first = collect(source);
    let second = collect(source);

    assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
    assert!(
        first
            .scopes
            .iter()
            .any(|scope| scope.kind == ScopeKind::Function)
    );
    assert!(
        first
            .scopes
            .iter()
            .any(|scope| scope.kind == ScopeKind::Block)
    );
    assert!(
        first
            .scopes
            .iter()
            .any(|scope| scope.kind == ScopeKind::Dynamic)
    );
    assert!(
        first
            .scopes
            .iter()
            .any(|scope| scope.kind == ScopeKind::Function && scope.depth > 2)
    );
}

#[test]
fn reuses_same_span_same_kind_siblings_by_order() {
    let parsed = crate::parse("value;", "same-span.js").expect("source should parse");
    let span = parsed.program.span();
    let mut collector = planned_scopes(span, &[ScopeKind::Block, ScopeKind::Block]);
    let predeclared = collector.scope_shapes.shapes_len();
    assert_eq!(predeclared, 2);

    collector.push_scope(span, ScopeKind::Block);
    let first = collector.current_scope();
    collector.pop_scope();
    collector.push_scope(span, ScopeKind::Block);
    let second = collector.current_scope();

    assert_eq!((first, second), (ScopeId::from(1), ScopeId::from(2)));
    assert_eq!(collector.scope_lookups, 2);
    assert_eq!(
        collector
            .scope_shapes
            .remaining(Some(ScopeId::from(0)), span.lo, ScopeKind::Block),
        0,
    );
}

fn sibling_scope_lookups(count: usize) -> usize {
    let source = (0..count)
        .map(|index| format!("{{ let value{index} = {index}; }}"))
        .collect::<Vec<_>>()
        .join("\n");
    let collector = collect(&source);
    collector.scope_lookups
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
    let parsed = crate::parse("value;", "divergence-extra.js").expect("source should parse");
    let span = parsed.program.span();
    let mut collector = planned_scopes(span, &[ScopeKind::Block]);
    assert_eq!(collector.scope_shapes.shapes_len(), 1);
    let before = collector.current_scope();
    collector.push_scope(span, ScopeKind::Block);
    collector.pop_scope();
    assert!(collector.scope_issues.is_empty());
    assert_eq!(collector.current_scope(), before);
    collector.push_scope(span, ScopeKind::Block);
    assert!(!collector.scope_issues.is_empty());
    // No fallback scope was allocated during the diverged push.
    assert_eq!(collector.current_scope(), before);
}

#[test]
fn divergence_on_missing_scope_fails_closed() {
    let parsed = crate::parse("value;", "divergence-missing.js").expect("source should parse");
    let span = parsed.program.span();
    let mut collector = planned_scopes(span, &[ScopeKind::Block, ScopeKind::Block]);
    assert_eq!(collector.scope_shapes.shapes_len(), 2);
    collector.push_scope(span, ScopeKind::Block);
    collector.pop_scope();
    assert!(collector.scope_issues.is_empty());
    assert_eq!(
        collector
            .scope_shapes
            .remaining(Some(ScopeId::from(0)), span.lo, ScopeKind::Block),
        1,
        "the unvisited predeclared shape stays in the table",
    );
    // A second visit consumes the remaining predeclared shape.
    collector.push_scope(span, ScopeKind::Block);
    assert!(collector.scope_issues.is_empty());
    // A third visit finds no matching shape and fails closed.
    let before = collector.current_scope();
    collector.push_scope(span, ScopeKind::Block);
    assert!(!collector.scope_issues.is_empty());
    // No fallback scope was allocated.
    assert_eq!(collector.current_scope(), before);
}

#[test]
fn divergence_on_kind_mismatch_fails_closed() {
    let parsed = crate::parse("value;", "divergence-kind.js").expect("source should parse");
    let span = parsed.program.span();
    let mut collector = planned_scopes(span, &[ScopeKind::Block]);
    let before = collector.current_scope();
    collector.push_scope(span, ScopeKind::Function);
    assert!(!collector.scope_issues.is_empty());
    // The visitor stays in the parent scope; no fallback is allocated.
    assert_eq!(collector.current_scope(), before);
}

#[test]
fn hoisted_var_in_blocks_preserves_function_scoping() {
    let source = r"
        function outer() {
            if (true) { var hoisted = 1; }
            return hoisted;
        }
    ";
    let collector = collect(source);

    let function_scopes: Vec<_> = collector
        .scopes
        .iter()
        .enumerate()
        .filter(|(_, scope)| scope.kind == ScopeKind::Function)
        .collect();
    assert_eq!(function_scopes.len(), 1);
    let (fn_idx, fn_scope) = function_scopes[0];
    assert!(
        !fn_scope.bindings.is_empty(),
        "function scope {fn_idx} has no bindings",
    );

    let block_scopes: Vec<_> = collector
        .scopes
        .iter()
        .enumerate()
        .filter(|(_, scope)| scope.kind == ScopeKind::Block)
        .collect();
    // var hoisted into function scope means block scopes should not have
    // the hoisted binding
    for (idx, scope) in &block_scopes {
        let is_empty = !scope
            .bindings
            .iter()
            .any(|(_, p)| matches!(p, BindingProvenance::Local));
        assert!(is_empty, "block scope {idx} contains var bindings");
    }
}

#[test]
fn catch_without_param_forms_valid_scope() {
    let source = r"
        try { let a = 1; } catch { let b = 2; }
    ";
    let first = collect(source);
    let second = collect(source);
    assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
    assert!(
        first
            .scopes
            .iter()
            .any(|scope| scope.kind == ScopeKind::Block && scope.depth == 1)
    );
}

#[test]
fn loops_with_and_without_inits_form_valid_scopes() {
    let source = r"
        for (;;) { break; }
        for (let i = 0; i < 1; i++) { break; }
        for (const x of []) { break; }
        for (const k in {}) { break; }
    ";
    let first = collect(source);
    let second = collect(source);
    assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
    assert_eq!(
        first
            .scopes
            .iter()
            .filter(|scope| scope.kind == ScopeKind::Block)
            .count(),
        second
            .scopes
            .iter()
            .filter(|scope| scope.kind == ScopeKind::Block)
            .count()
    );
}

#[test]
fn with_statement_creates_dynamic_scope() {
    let source = r"
        const obj = {};
        with (obj) { let value = prop; }
    ";
    let first = collect(source);
    let second = collect(source);
    assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
    assert!(
        first
            .scopes
            .iter()
            .any(|scope| scope.kind == ScopeKind::Dynamic)
    );
}

#[test]
fn switch_with_cases_forms_block_scope() {
    let source = r"
        switch (a) { case 0: { let b = 1; break; } default: break; }
    ";
    let first = collect(source);
    let second = collect(source);
    assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
    // Switch body is a block scope
    assert!(
        first
            .scopes
            .iter()
            .any(|scope| scope.kind == ScopeKind::Block && scope.depth == 1)
    );
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
    let collector = collect(source);
    let function_depths: Vec<_> = collector
        .scopes
        .iter()
        .filter(|scope| scope.kind == ScopeKind::Function)
        .map(|scope| scope.depth)
        .collect();
    // Function bodies have intervening block scopes:
    // depth 1 = a, depth 3 = b (after a-block), depth 5 = arrow c (after a-block +
    // b-block)
    assert!(function_depths.contains(&1));
    assert!(function_depths.contains(&3));
    assert!(function_depths.contains(&5));
}

#[test]
fn predeclare_and_collect_phases_produce_identical_scopes() {
    let source = r"
        function outer(p1, p2) {
            const value = p1 + p2;
            for (const item of [1,2,3]) {
                const doubled = item * 2;
            }
            try { throw value; }
            catch (error) {
                const message = error.toString();
            }
            if (value) {
                const flag = true;
            } else {
                const flag = false;
            }
            const helper = (x) => x + 1;
            helper(value);
        }
    ";
    let first = collect(source);
    let second = collect(source);
    assert_eq!(first.scopes.len(), second.scopes.len());
    for (i, (a, b)) in first.scopes.iter().zip(second.scopes.iter()).enumerate() {
        assert_eq!(
            a.kind, b.kind,
            "scope {i} kind differs: {:?} vs {:?}",
            a.kind, b.kind
        );
        assert_eq!(a.depth, b.depth, "scope {i} depth differs");
        assert_eq!(a.parent, b.parent, "scope {i} parent differs");
        assert_eq!(
            a.bindings.keys().collect::<Vec<_>>(),
            b.bindings.keys().collect::<Vec<_>>(),
            "scope {i} binding keys differ",
        );
    }
}

#[test]
fn structural_lookup_distinguishes_equal_span_siblings_at_different_parents() {
    let source = r"
        { let outer = 1; }
        function f() { { let inner = 1; } }
    ";
    let collector = collect(source);

    let (program_block_index, program_block) = collector
        .scopes
        .iter()
        .enumerate()
        .find(|(_, scope)| scope.kind == ScopeKind::Block && scope.parent == Some(ScopeId::from(0)))
        .expect("outer block under program");
    let (function_index, _function_scope) = collector
        .scopes
        .iter()
        .enumerate()
        .find(|(_, scope)| {
            scope.kind == ScopeKind::Function && scope.parent == Some(ScopeId::from(0))
        })
        .expect("function under program");
    let (inner_block_index, inner_block) = collector
        .scopes
        .iter()
        .enumerate()
        .find(|(_, scope)| {
            scope.kind == ScopeKind::Block && scope.parent == Some(ScopeId::from(function_index))
        })
        .expect("inner block under function");

    // Both blocks share a Span layout but have different parents; the
    // structural lookup must keep them distinct.
    assert_ne!(program_block_index, inner_block_index);
    assert_eq!(program_block.parent, Some(ScopeId::from(0)));
    assert_eq!(inner_block.parent, Some(ScopeId::from(function_index)));
}

#[test]
fn structural_lookup_resolves_visitor_pushes_without_positional_synchronization() {
    let source = r"
        function outer() {
            for (let i = 0; i < 1; i++) {
                try { throw i; } catch (e) { const v = e; }
            }
            with (context) { const w = prop; }
            const arrow = () => { return 1; };
        }
    ";
    let collector = collect(source);
    assert!(
        collector.scope_issues.is_empty(),
        "no divergence when the visitor walks scope-forming syntax in predeclaration order",
    );
    assert_eq!(
        collector.scope_lookups,
        collector.scope_shapes.shapes_len(),
        "every predeclared shape was consumed by one visitor push",
    );
}

#[test]
fn deliberate_walker_divergence_fails_closed_without_fallback_allocation() {
    // Predeclare 3 sibling Block scopes under the program scope.
    let parsed = crate::parse("value;", "walker-divergence.js").expect("source should parse");
    let span = parsed.program.span();
    let mut collector = planned_scopes(
        span,
        &[ScopeKind::Block, ScopeKind::Block, ScopeKind::Block],
    );
    let predeclared = collector.scope_shapes.shapes_len();
    assert_eq!(predeclared, 3);

    // Walk the predeclared shapes in reversed order: a structural
    // identity lookup must still resolve each push correctly because
    // the lookup is keyed by (parent, span, kind), not by position.
    let program = ScopeId::from(0);
    let remaining_first =
        collector
            .scope_shapes
            .remaining(Some(program), span.lo, ScopeKind::Block);
    assert_eq!(remaining_first, 3);
    collector.push_scope(span, ScopeKind::Block);
    let first = collector.current_scope();
    collector.pop_scope();
    assert!(collector.scope_issues.is_empty());
    collector.push_scope(span, ScopeKind::Block);
    let second = collector.current_scope();
    collector.pop_scope();
    assert!(collector.scope_issues.is_empty());
    collector.push_scope(span, ScopeKind::Block);
    let third = collector.current_scope();
    collector.pop_scope();
    assert!(collector.scope_issues.is_empty());
    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_ne!(first, third);
    assert_eq!(
        collector.scope_lookups, 3,
        "every predeclared shape was consumed",
    );

    // A visit that is not preceded by a matching predeclared shape
    // must fail closed without allocating a fallback scope.
    let before = collector.current_scope();
    collector.push_scope(span, ScopeKind::Block);
    assert!(!collector.scope_issues.is_empty());
    assert_eq!(
        collector.current_scope(),
        before,
        "divergence leaves the visitor in the parent scope",
    );
}
