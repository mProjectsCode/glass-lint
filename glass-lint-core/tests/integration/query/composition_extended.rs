use super::*;

#[test]
fn predicate_alternatives_at_limit_succeeds() {
    let values: Vec<String> = (0..256).map(|i| format!("val{i}")).collect();
    let m = ValueMatcher::static_string().equals_any(values).unwrap();
    // Should not panic or error — canonicalization handles up to any size
    let _ = m;
}

#[test]
fn predicate_alternatives_limit_plus_one_fails_at_construction() {
    let values: Vec<String> = (0..=256).map(|i| format!("val{i}")).collect();
    assert!(matches!(
        ValueMatcher::static_string().equals_any(values),
        Err(QueryBuildError::CollectionTooLarge(_, 257))
    ));
}

#[test]
fn query_roots_boundary_succeeds() {
    let queries: Vec<QueryDecl> = (0..256)
        .map(|i| EventQuery::call_global(format!("fn{i}")).map(EventQuery::into_query))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(queries.len(), 256);
}

#[test]
fn query_roots_limit_plus_one_is_rejected_at_authoring() {
    let mut builder = rule("test.too-many-roots");
    for i in 0..=256 {
        builder = builder.query(EventQuery::call_global(format!("fn{i}")));
    }

    let error = builder.build().unwrap_err();
    assert!(error.to_string().contains("query roots"));
}

// ── QueryBuildError validation tests ───────────────────────────────────
#[test]
fn empty_lifecycle_sources_rejected() {
    let err = LifecycleQuery::catalog_builder("test")
        .completion(glass_lint_core::rules::LifecycleCompletion::configuration())
        .build()
        .unwrap_err();
    assert!(matches!(err, QueryBuildError::MissingLifecycleSources));
}

#[test]
fn empty_lifecycle_evidence_symbol_rejected() {
    let err = LifecycleQuery::catalog_builder(" ")
        .source(glass_lint_core::rules::EventQuery::member_call_rooted(
            "document.create",
        ))
        .completion(glass_lint_core::rules::LifecycleCompletion::any_sink([
            glass_lint_core::rules::LifecycleSink::argument_of_member("sink", 0),
        ]))
        .build()
        .unwrap_err();
    assert!(matches!(err, QueryBuildError::EmptyEvidenceSymbol));
}

#[test]
fn invalid_scope_package_rejected() {
    let err = EventQuery::call_package("  ", "export");
    assert!(matches!(err, Err(QueryBuildError::InvalidScopePackage(_))));
}

#[test]
fn excessive_constraints_same_index_rejected() {
    // Add one more predicate than the per-argument construction limit.
    let mut q = EventQuery::call_global("fetch").unwrap();
    for _ in 0..2049 {
        match q.with_arg(0, ValueMatcher::static_string()) {
            Ok(next) => q = next,
            Err(e) => {
                assert!(
                    matches!(e, QueryBuildError::ExcessivePredicates { .. }),
                    "expected ExcessivePredicates, got: {e:?}"
                );
                return;
            }
        }
    }
    panic!("expected ExcessiveConstraints at constraint 2049");
}

#[test]
fn argument_group_limit_plus_one_fails_at_construction() {
    let mut query = EventQuery::call_global("fetch").unwrap();
    for index in 0..64 {
        query = query
            .with_arg(index, ValueMatcher::static_string())
            .unwrap();
    }
    assert!(matches!(
        query.with_arg(64, ValueMatcher::static_string()),
        Err(QueryBuildError::ExcessiveArgumentGroups(65))
    ));
}

#[test]
fn equivalent_argument_order_produces_equal_matchers() {
    let a = ValueMatcher::static_string()
        .equals_any(["b", "a", "c"])
        .unwrap();
    let b = ValueMatcher::static_string()
        .equals_any(["c", "a", "b"])
        .unwrap();
    assert_eq!(a, b, "canonicalized equals_any should be order-independent");
}

#[test]
fn equivalent_starts_with_order_produces_equal_matchers() {
    let a = ValueMatcher::static_string()
        .starts_with_any(["z", "a"])
        .unwrap();
    let b = ValueMatcher::static_string()
        .starts_with_any(["a", "z"])
        .unwrap();
    assert_eq!(
        a, b,
        "canonicalized starts_with_any should be order-independent"
    );
}

#[test]
fn equivalent_contains_any_order_produces_equal_matchers() {
    let a = ValueMatcher::static_string()
        .contains_any(["secret", "token"])
        .unwrap();
    let b = ValueMatcher::static_string()
        .contains_any(["token", "secret"])
        .unwrap();
    assert_eq!(
        a, b,
        "canonicalized contains_any should be order-independent"
    );
}

#[test]
fn equivalent_contains_all_order_produces_equal_matchers() {
    let a = ValueMatcher::static_string()
        .contains_all(["b", "a"])
        .unwrap();
    let b = ValueMatcher::static_string()
        .contains_all(["a", "b"])
        .unwrap();
    assert_eq!(
        a, b,
        "canonicalized contains_all should be order-independent"
    );
}

#[test]
fn deduplicated_alternatives_are_removed() {
    // equals_any with duplicates should deduplicate to one element.
    let m = ValueMatcher::static_string()
        .equals_any(["a", "a", "a"])
        .unwrap();
    let expected = ValueMatcher::static_string().try_equals("a").unwrap();
    assert_eq!(m, expected, "duplicates in equals_any should be removed");
}

#[test]
fn query_modifiers_do_not_silently_ignore_non_event_expressions() {
    // Verify that with_arg is only available on EventQuery, not on
    // composed QueryDecl — meaning it cannot silently ignore
    // non-Event expressions.
    let event_query = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap();
    let modified = event_query.into_query();
    assert!(
        modified.expression().diagnostic_name() == "event",
        "with_arg should produce an Event expression, got: {:?}",
        modified.expression()
    );

    // Also verify that invalid index produces a build error, not a panic.
    let err = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(300, ValueMatcher::static_string());
    assert!(
        err.is_err(),
        "excessive argument index must produce an error"
    );
}
