use super::*;
#[test]
fn normalized_query_compiles_through_full_pipeline() {
    let d = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize::normalize_query_decl(&d).unwrap();
    let _plan = crate::api::compiler::physical::plan_normalized(&nq).unwrap();
}

#[test]
fn duplicate_filters_do_not_duplicate_work_or_evidence() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().try_equals("/api").unwrap())
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().try_equals("/api").unwrap())
        .unwrap();
    let nq = normalize::normalize_query_decl(&eq.into_query()).unwrap();
    match nq.root() {
        NormalizedRoot::Event(ev) => {
            assert_eq!(
                ev.arguments().to_flat_vec().len(),
                1,
                "duplicate constraints must be deduplicated"
            );
        }
        other => panic!("expected Event, got {other:?}"),
    }
}

#[test]
fn duplicate_filters_in_all_are_deduplicated() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().try_equals("/api").unwrap())
        .unwrap();
    let req =
        EventRequirement::argument(0, ValueMatcher::static_string().try_equals("/api").unwrap())
            .unwrap();
    let d = QueryDecl::all(Ok(eq), [Ok(req)]).unwrap();
    let nq = normalize::normalize_query_decl(&d).unwrap();
    match nq.root() {
        NormalizedRoot::Event(ev) => {
            assert_eq!(
                ev.arguments().to_flat_vec().len(),
                1,
                "duplicate constraints from All branches must be deduplicated"
            );
        }
        other => panic!("expected Event, got {other:?}"),
    }
}

#[test]
fn distinct_lifecycle_conditions_never_compare_as_same_ordering_key() {
    let source_a = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::MemberCall {
            member: SymbolPath::from("document.createElement"),
        },
        IdentitySpec::Rooted {
            path: SymbolPath::from("document.createElement"),
        },
        vec![],
    );
    let source_b = EventQuery::from_parts_for_test(
        VarId::new(1),
        EventSpec::MemberCall {
            member: SymbolPath::from("doc.createElement"),
        },
        IdentitySpec::Rooted {
            path: SymbolPath::from("doc.createElement"),
        },
        vec![],
    );

    let lc_a = lifecycle(
        "test-a",
        vec![source_a],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("src", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        crate::api::rule::LifecycleCompletion::configuration(),
    );

    let lc_b = lifecycle(
        "test-b",
        vec![source_b],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("href", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        crate::api::rule::LifecycleCompletion::configuration(),
    );

    let d_a = QueryDecl::from_parts_for_test(
        QueryExpr::lifecycle(lc_a),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::CallArgument,
            symbol: "test-a".into(),
        },
    );
    let d_b = QueryDecl::from_parts_for_test(
        QueryExpr::lifecycle(lc_b),
        EmissionDecl {
            primary_var: VarId::new(1),
            kind: MatchKind::CallArgument,
            symbol: "test-b".into(),
        },
    );

    let nq_a = normalize_ok(&d_a);
    let nq_b = normalize_ok(&d_b);
    assert_ne!(
        nq_a, nq_b,
        "lifecycle queries with different conditions must not compare equal"
    );
}

#[test]
fn unknown_sensitive_forms_are_not_over_simplified() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::any_value())
        .unwrap();
    let nq = normalize::normalize_query_decl(&eq.into_query()).unwrap();
    match nq.root() {
        NormalizedRoot::Event(ev) => {
            let flat = ev.arguments().to_flat_vec();
            assert_eq!(
                flat.len(),
                1,
                "AnyValue constraint must be preserved through normalization"
            );
            let matcher = flat[0].predicate();
            assert_eq!(
                matcher.kind(),
                &crate::api::rule::ArgumentMatcherKind::Value(
                    crate::api::rule::ValueMatcher::any_value()
                )
            );
        }
        other => panic!("expected Event, got {other:?}"),
    }
}
