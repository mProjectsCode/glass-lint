use super::*;
#[test]
fn single_branch_any_is_preserved() {
    let d = decl(
        QueryExpr::any(AnyExpr::new(vec![event(0, "a")]).unwrap()),
        0,
        "test",
    );
    let nq = normalize_ok(&d);
    assert!(matches!(nq.root(), NormalizedRoot::Event(_)));
}

#[test]
fn single_branch_all_is_normalized_to_event() {
    let all = AllExpr::new(vec![event(0, "a")]).unwrap();
    let d = decl(QueryExpr::all(all), 0, "a");
    let nq = normalize_ok(&d);
    assert!(matches!(nq.root(), NormalizedRoot::Event(_)));
}

#[test]
fn normalize_preserves_emission_kind_and_symbol() {
    let d = decl(event(0, "fetch"), 0, "fetch");
    let nq = normalize_ok(&d);
    assert_eq!(nq.emission().symbol(), "fetch");
    assert_eq!(nq.emission().kind(), MatchKind::Call);
}

#[test]
fn reversed_argument_orders_normalize_equally() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(1, ValueMatcher::static_string())
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().equals("/api"))
        .unwrap();
    let d = eq.into_query();
    let nq = normalize_ok(&d);
    match nq.root() {
        NormalizedRoot::Event(ev) => {
            let flat = ev.arguments.to_flat_vec();
            assert_eq!(flat.len(), 2);
            assert_eq!(flat[0].index(), 0);
            assert_eq!(flat[1].index(), 1);
        }
        other => panic!("expected Event, got {other:?}"),
    }
}

#[test]
fn reversed_alternative_order_normalizes_equally() {
    let a_branches = vec![event(0, "fetch"), event(1, "navigate")];
    let b_branches = vec![event(1, "navigate"), event(0, "fetch")];
    let a_d = decl(QueryExpr::any(AnyExpr::new(a_branches).unwrap()), 0, "test");
    let b_d = decl(QueryExpr::any(AnyExpr::new(b_branches).unwrap()), 0, "test");
    let a_nq = normalize_ok(&a_d);
    let b_nq = normalize_ok(&b_d);
    assert_eq!(a_nq, b_nq);
}

#[test]
fn no_normalized_all_variant_exists() {
    let event_query = EventQuery::call_global("fetch").unwrap();
    let d = QueryDecl::all(Ok(event_query), []).unwrap();
    let nq = normalize_ok(&d);
    match nq.root() {
        NormalizedRoot::Event(_) => {}
        _ => panic!("expected Event after normalization of single-branch All"),
    }
}

#[test]
fn alpha_equivalent_variable_ids_normalize_equally() {
    let a_branches = vec![event(10, "fetch"), event(20, "navigate")];
    let b_branches = vec![event(30, "fetch"), event(40, "navigate")];
    let a_d = decl(
        QueryExpr::any(AnyExpr::new(a_branches).unwrap()),
        10,
        "test",
    );
    let b_d = decl(
        QueryExpr::any(AnyExpr::new(b_branches).unwrap()),
        30,
        "test",
    );
    let a_nq = normalize_ok(&a_d);
    let b_nq = normalize_ok(&b_d);
    assert_eq!(a_nq, b_nq);
    match a_nq.root() {
        NormalizedRoot::Any(branches) => {
            let mut slots: Vec<u32> = branches
                .iter()
                .map(|b| match b {
                    NormalizedRoot::Event(ev) => ev.slot,
                    _ => u32::MAX,
                })
                .collect();
            slots.sort_unstable();
            assert_eq!(slots, vec![0, 1], "slots should be dense 0..n");
        }
        other => panic!("expected Any, got {other:?}"),
    }
}

#[test]
fn alpha_equivalent_single_event_normalizes_equally() {
    let a = decl(event(5, "fetch"), 5, "fetch");
    let b = decl(event(99, "fetch"), 99, "fetch");
    assert_eq!(normalize_ok(&a), normalize_ok(&b));
    match normalize_ok(&a).root() {
        NormalizedRoot::Event(ev) => assert_eq!(ev.slot, 0, "slot should be 0 after alpha"),
        other => panic!("expected Event, got {other:?}"),
    }
}

#[test]
fn exact_and_prefix_contradiction_is_detected() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().equals("foo"))
        .unwrap()
        .with_arg(
            0,
            ValueMatcher::static_string()
                .starts_with_any(["bar"])
                .unwrap(),
        )
        .unwrap();
    let result = normalize::normalize_query_decl(&eq.into_query());
    assert!(
        matches!(
            result,
            Err(
                crate::api::compiler::validate::QueryCompileError::ContradictoryPredicate {
                    detail: crate::api::compiler::validate::ContradictionKind::StaticExactAndPrefix,
                    ..
                }
            )
        ),
        "expected StaticExactAndPrefix contradiction, got {result:?}"
    );
}

#[test]
fn exact_and_non_contradictory_prefix_passes() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().equals("foobar"))
        .unwrap()
        .with_arg(
            0,
            ValueMatcher::static_string()
                .starts_with_any(["foo"])
                .unwrap(),
        )
        .unwrap();
    assert!(normalize::normalize_query_decl(&eq.into_query()).is_ok());
}

#[test]
fn three_pairwise_overlapping_exact_sets_can_still_be_contradictory() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(
            0,
            ValueMatcher::static_string()
                .equals_any(["a", "b"])
                .unwrap(),
        )
        .unwrap()
        .with_arg(
            0,
            ValueMatcher::static_string()
                .equals_any(["b", "c"])
                .unwrap(),
        )
        .unwrap()
        .with_arg(
            0,
            ValueMatcher::static_string()
                .equals_any(["a", "c"])
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        normalize::normalize_query_decl(&eq.into_query()),
        Err(
            crate::api::compiler::validate::QueryCompileError::ContradictoryPredicate {
                detail: crate::api::compiler::validate::ContradictionKind::StaticExactValues,
                ..
            }
        )
    ));
}

#[test]
fn exact_value_must_satisfy_every_prefix_conjunct() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().equals("foobar"))
        .unwrap()
        .with_arg(
            0,
            ValueMatcher::static_string()
                .starts_with_any(["foo"])
                .unwrap(),
        )
        .unwrap()
        .with_arg(
            0,
            ValueMatcher::static_string()
                .starts_with_any(["bar"])
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        normalize::normalize_query_decl(&eq.into_query()),
        Err(
            crate::api::compiler::validate::QueryCompileError::ContradictoryPredicate {
                detail: crate::api::compiler::validate::ContradictionKind::StaticExactAndPrefix,
                ..
            }
        )
    ));
}

#[test]
fn incompatible_prefix_only_conjunction_is_contradictory() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(
            0,
            ValueMatcher::static_string()
                .starts_with_any(["ab"])
                .unwrap(),
        )
        .unwrap()
        .with_arg(
            0,
            ValueMatcher::static_string()
                .starts_with_any(["cd"])
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        normalize::normalize_query_decl(&eq.into_query()),
        Err(
            crate::api::compiler::validate::QueryCompileError::ContradictoryPredicate {
                detail: crate::api::compiler::validate::ContradictionKind::StaticExactAndPrefix,
                ..
            }
        )
    ));
}

#[test]
fn empty_exact_set_is_rejected_at_construction() {
    let empty: Vec<&str> = vec![];
    assert!(matches!(
        ValueMatcher::static_string().equals_any(empty),
        Err(crate::api::rule::QueryBuildError::EmptyCollection(_))
    ));
}

#[test]
fn empty_contains_any_set_is_rejected_at_construction() {
    let empty: Vec<String> = vec![];
    assert!(matches!(
        ValueMatcher::static_string().contains_any(empty),
        Err(crate::api::rule::QueryBuildError::EmptyCollection(_))
    ));
}

#[test]
fn normalized_root_slots_are_dense_after_alpha_renumber() {
    let d = decl(event(99, "fetch"), 99, "fetch");
    let nq = normalize_ok(&d);
    match nq.root() {
        NormalizedRoot::Event(ev) => {
            assert_eq!(ev.slot, 0, "slot should be renumbered to 0");
        }
        other => panic!("expected Event, got {other:?}"),
    }
}

#[test]
fn normalized_any_branches_have_dense_slots() {
    let branches = vec![event(10, "a"), event(20, "b"), event(5, "c")];
    let d = decl(QueryExpr::any(AnyExpr::new(branches).unwrap()), 10, "test");
    let nq = normalize_ok(&d);
    match nq.root() {
        NormalizedRoot::Any(roots) => {
            let mut slots: Vec<u32> = roots
                .iter()
                .map(|r| match r {
                    NormalizedRoot::Event(ev) => ev.slot,
                    _ => u32::MAX,
                })
                .collect();
            slots.sort_unstable();
            assert_eq!(slots, vec![0, 1, 2], "slots should be dense 0..2");
        }
        other => panic!("expected Any, got {other:?}"),
    }
}
