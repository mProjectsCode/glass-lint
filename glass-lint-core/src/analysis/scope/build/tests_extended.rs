use super::*;

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
    with_collector(source, |first| {
        let second = with_collector(source, |second| {
            second
                .lexical
                .scopes
                .iter()
                .map(|scope| {
                    (
                        scope.kind(),
                        scope.depth(),
                        scope.parent(),
                        scope
                            .binding_names()
                            .map(|name| format!("{name:?}"))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(first.lexical.scopes.len(), second.len());
        for (i, (a, b)) in first.lexical.scopes.iter().zip(second).enumerate() {
            assert_eq!(
                a.kind(),
                b.0,
                "scope {i} kind differs: {:?} vs {:?}",
                a.kind(),
                b.0
            );
            assert_eq!(a.depth(), b.1, "scope {i} depth differs");
            assert_eq!(a.parent(), b.2, "scope {i} parent differs");
            let mut a_keys: Vec<_> = a.binding_names().map(|name| format!("{name:?}")).collect();
            a_keys.sort();
            let mut b_keys = b.3;
            b_keys.sort();
            assert_eq!(a_keys, b_keys, "scope {i} binding keys differ");
        }
    });
}

#[test]
fn structural_lookup_distinguishes_equal_span_siblings_at_different_parents() {
    let source = r"
        { let outer = 1; }
        function f() { { let inner = 1; } }
    ";
    with_collector(source, |collector| {
        let (program_block_index, program_block) = collector
            .lexical
            .scopes
            .iter()
            .enumerate()
            .find(|(_, scope)| {
                scope.kind() == ScopeKind::Block && scope.parent() == Some(ScopeId::from_test(0))
            })
            .expect("outer block under program");
        let (function_index, _function_scope) = collector
            .lexical
            .scopes
            .iter()
            .enumerate()
            .find(|(_, scope)| {
                scope.kind() == ScopeKind::Function && scope.parent() == Some(ScopeId::from_test(0))
            })
            .expect("function under program");
        let (inner_block_index, inner_block) = collector
            .lexical
            .scopes
            .iter()
            .enumerate()
            .find(|(_, scope)| {
                scope.kind() == ScopeKind::Block
                    && scope.parent() == Some(ScopeId::from_test(function_index))
            })
            .expect("inner block under function");

        // Both blocks share a Span layout but have different parents; the
        // structural lookup must keep them distinct.
        assert_ne!(program_block_index, inner_block_index);
        assert_eq!(program_block.parent(), Some(ScopeId::from_test(0)));
        assert_eq!(
            inner_block.parent(),
            Some(ScopeId::from_test(function_index))
        );
    });
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
    with_collector(source, |collector| {
        assert!(
            !collector.artifacts.has_issues(),
            "no divergence when the visitor walks scope-forming syntax in predeclaration order",
        );
        assert_eq!(
            collector.scope_lookups,
            collector.lexical.scope_shapes.shapes_len(),
            "every predeclared shape was consumed by one visitor push",
        );
    });
}

#[test]
fn deliberate_walker_divergence_fails_closed_without_fallback_allocation() {
    // Predeclare 3 sibling Block scopes under the program scope.
    let parsed =
        crate::parse_test_source("value;", "walker-divergence.js").expect("source should parse");
    let span = parsed.program.span();
    with_planned_scopes(
        span,
        &[ScopeKind::Block, ScopeKind::Block, ScopeKind::Block],
        |collector| {
            let predeclared = collector.lexical.scope_shapes.shapes_len();
            assert_eq!(predeclared, 3);

            // Walk the predeclared shapes in reversed order: a structural
            // identity lookup must still resolve each push correctly because
            // the lookup is keyed by (parent, span, kind), not by position.
            let program = ScopeId::from_test(0);
            let remaining_first =
                collector
                    .lexical
                    .scope_shapes
                    .remaining(Some(program), span.lo, ScopeKind::Block);
            assert_eq!(remaining_first, 3);
            collector.push_scope(span, ScopeKind::Block);
            let first = collector.current_scope();
            collector.pop_scope();
            assert!(!collector.artifacts.has_issues());
            collector.push_scope(span, ScopeKind::Block);
            let second = collector.current_scope();
            collector.pop_scope();
            assert!(!collector.artifacts.has_issues());
            collector.push_scope(span, ScopeKind::Block);
            let third = collector.current_scope();
            collector.pop_scope();
            assert!(!collector.artifacts.has_issues());
            assert_ne!(first, second);
            assert_ne!(second, third);
            assert_ne!(first, third);
            assert_eq!(
                collector.scope_lookups, 3,
                "every predeclared shape was consumed",
            );

            // A visit that is not preceded by a matching predeclared shape
            // must fail closed without allocating a fallback scope.
            collector.push_scope(span, ScopeKind::Block);
            assert!(collector.artifacts.has_issues());
            assert!(collector.current_scope().is_none());
        },
    );
}
