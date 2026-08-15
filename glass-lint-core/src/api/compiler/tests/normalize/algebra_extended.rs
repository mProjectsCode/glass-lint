use super::*;

#[test]
fn lifecycle_has_flow_requirements() {
    let source = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::MemberCall {
            member: SymbolPath::from("document.createElement"),
        },
        IdentitySpec::Rooted {
            path: SymbolPath::from("document.createElement"),
        },
        vec![],
    );
    let lc = lifecycle(
        "test",
        vec![source],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("src", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration()),
    );
    let d = QueryDecl::from_parts_for_test(
        QueryExpr::lifecycle(lc),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::CallArgument,
            symbol: "test".into(),
        },
    );
    let nq = normalize_ok(&d);
    let req = super::plan_requirements(&nq);
    assert!(req.flow().local(), "lifecycle should need local flow");
    assert!(
        req.flow().cross_call(),
        "lifecycle should need cross-call flow"
    );
}

#[test]
fn global_query_has_only_calls_requirement() {
    let d = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize_ok(&d);
    let req = super::plan_requirements(&nq);
    assert!(req.value_resolution().is_empty());
    assert!(!req.flow().local());
    assert!(!req.flow().cross_call());
    assert!(!req.needs_project_overlay());
}

#[test]
fn lifecycle_is_not_flattened_or_sorted() {
    let source = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::MemberCall {
            member: SymbolPath::from("document.createElement"),
        },
        IdentitySpec::Rooted {
            path: SymbolPath::from("document.createElement"),
        },
        vec![],
    );
    let lc = lifecycle(
        "test",
        vec![source],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration()),
    );
    let d = QueryDecl::from_parts_for_test(
        QueryExpr::lifecycle(lc),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::CallArgument,
            symbol: "test".into(),
        },
    );
    let nq = normalize_ok(&d);
    assert!(matches!(nq.root(), NormalizedRoot::Lifecycle(_)));
}
