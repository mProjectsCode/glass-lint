use super::*;
#[test]
fn simple_event_normalizes_to_event_root() {
    let d = decl(event(0, "fetch"), 0, "fetch");
    let nq = normalize_ok(&d);
    assert!(matches!(nq.root(), NormalizedRoot::Event(_)));
}

#[test]
fn lifecycle_normalizes_to_lifecycle_root() {
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
        "remote-script",
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
            symbol: "remote-script".into(),
        },
    );
    let nq = normalize_ok(&d);
    assert!(matches!(nq.root(), NormalizedRoot::Lifecycle(_)));
}

#[test]
fn flattens_nested_any() {
    let inner = AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap();
    let outer = AnyExpr::new(vec![event(2, "c"), QueryExpr::any(inner), event(3, "d")]).unwrap();
    let d = decl(QueryExpr::any(outer), 0, "test");
    let nq = normalize_ok(&d);
    match nq.root() {
        NormalizedRoot::Any(branches) => {
            assert_eq!(branches.len(), 4);
        }
        other => panic!("expected Any, got {other:?}"),
    }
}

#[test]
fn does_not_flatten_any_into_all() {
    let inner_any = AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap();
    let all = AllExpr::new(vec![event(2, "c"), QueryExpr::any(inner_any)]).unwrap();
    let d = decl(QueryExpr::all(all), 2, "test");
    let result = normalize::normalize_query_decl(&d);
    assert!(result.is_err());
}

#[test]
fn deduplicates_identical_branches_in_any() {
    let branches = vec![event(0, "a"), event(0, "a"), event(1, "b")];
    let d = decl(QueryExpr::any(AnyExpr::new(branches).unwrap()), 0, "test");
    let nq = normalize_ok(&d);
    match nq.root() {
        NormalizedRoot::Any(branches) => {
            assert_eq!(branches.len(), 2);
        }
        other => panic!("expected Any, got {other:?}"),
    }
}

#[test]
fn branches_are_sorted_canonically() {
    let branches = vec![event(1, "z"), event(0, "a")];
    let d = decl(QueryExpr::any(AnyExpr::new(branches).unwrap()), 0, "test");
    let nq = normalize_ok(&d);
    match nq.root() {
        NormalizedRoot::Any(roots) => {
            assert_eq!(roots.len(), 2);
            assert_eq!(roots[0].slot_or_zero(), 0);
            assert_eq!(roots[1].slot_or_zero(), 1);
        }
        other => panic!("expected Any, got {other:?}"),
    }
}

trait SlotAccess {
    fn slot_or_zero(&self) -> u32;
}
impl SlotAccess for NormalizedRoot {
    fn slot_or_zero(&self) -> u32 {
        match self {
            Self::Event(ev) => ev.slot,
            _ => 0,
        }
    }
}

#[test]
fn equivalent_builder_forms_normalize_equally() {
    let a_branches = vec![event(0, "fetch"), event(1, "navigate")];
    let b_branches = vec![event(1, "navigate"), event(0, "fetch")];
    let a_expr = QueryExpr::any(AnyExpr::new(a_branches).unwrap());
    let b_expr = QueryExpr::any(AnyExpr::new(b_branches).unwrap());
    let a_d = decl(a_expr, 0, "test");
    let b_d = decl(b_expr, 0, "test");
    let a_nq = normalize_ok(&a_d);
    let b_nq = normalize_ok(&b_d);
    assert_eq!(a_nq, b_nq);
}

#[test]
fn normalization_is_idempotent() {
    let branches = vec![
        QueryExpr::any(AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap()),
        event(2, "c"),
    ];
    let d = decl(QueryExpr::any(AnyExpr::new(branches).unwrap()), 0, "test");
    let normalized = normalize_ok(&d);
    let slots = normalize::collect_normalized_slots(normalized.root());
    assert_eq!(slots, vec![0, 1, 2]);
    assert_eq!(
        normalized.requirements(),
        &PlanRequirements::for_root(normalized.root())
    );
    match normalized.root() {
        NormalizedRoot::Any(branches) => {
            assert!(
                branches
                    .iter()
                    .all(|branch| { !matches!(branch, NormalizedRoot::Any(_)) })
            );
        }
        other => panic!("expected normalized Any, got {other:?}"),
    }
}

#[test]
fn normalization_of_simple_event_is_idempotent() {
    let d = decl(event(0, "fetch"), 0, "fetch");
    let once = normalize_ok(&d);
    let twice = normalize_ok(&d);
    assert_eq!(once, twice);
}

#[test]
fn same_event_all_merges_into_one_normalized_event() {
    let event_query = EventQuery::call_global("fetch").unwrap();
    let req: Result<EventRequirement, _> =
        Ok(EventRequirement::argument(0, ValueMatcher::static_string()).unwrap());
    let d = QueryDecl::all(Ok(event_query), [req]).unwrap();
    let nq = normalize_ok(&d);
    match nq.root() {
        NormalizedRoot::Event(ev) => {
            assert_eq!(ev.arguments.to_flat_vec().len(), 1);
        }
        other => panic!("expected Event, got {other:?}"),
    }
}

#[test]
fn same_event_all_with_multiple_constraints_merges_all_constraints() {
    let event_query = EventQuery::call_global("fetch").unwrap();
    let reqs: Vec<Result<EventRequirement, _>> = vec![
        Ok(EventRequirement::argument(0, ValueMatcher::static_string()).unwrap()),
        Ok(EventRequirement::argument(
            1,
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        )
        .unwrap()),
    ];
    let d = QueryDecl::all(Ok(event_query), reqs).unwrap();
    let nq = normalize_ok(&d);
    match nq.root() {
        NormalizedRoot::Event(ev) => {
            assert_eq!(ev.arguments.to_flat_vec().len(), 2);
        }
        other => panic!("expected Event, got {other:?}"),
    }
}

#[test]
fn incompatible_event_kinds_in_all_produce_contradiction() {
    let a = QueryExpr::event(EventQuery::from_parts_for_test(
        VarId::new(7),
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        vec![],
    ));
    let b = QueryExpr::event(EventQuery::from_parts_for_test(
        VarId::new(7),
        EventSpec::Construct,
        IdentitySpec::Global {
            name: SmolStr::new("URL"),
        },
        vec![],
    ));
    let all = AllExpr::new(vec![a, b]).unwrap();
    let d = decl(QueryExpr::all(all), 0, "test");
    let result = normalize::normalize_query_decl(&d);
    let Err(crate::api::compiler::validate::QueryCompileError::ContradictoryPredicate {
        variable,
        detail,
    }) = result
    else {
        panic!("expected ContradictoryPredicate(EventKind), got {result:?}");
    };
    assert_eq!(variable, VarId::new(7));
    assert_eq!(
        detail,
        crate::api::compiler::validate::ContradictionKind::EventKind
    );
}

#[test]
fn incompatible_identities_in_all_produce_contradiction() {
    let a = QueryExpr::event(EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        vec![],
    ));
    let b = QueryExpr::event(EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("navigate"),
        },
        vec![],
    ));
    let all = AllExpr::new(vec![a, b]).unwrap();
    let d = decl(QueryExpr::all(all), 0, "test");
    let result = normalize::normalize_query_decl(&d);
    assert!(
        matches!(
            result,
            Err(
                crate::api::compiler::validate::QueryCompileError::ContradictoryPredicate {
                    detail: crate::api::compiler::validate::ContradictionKind::StrictIdentity,
                    ..
                }
            )
        ),
        "expected ContradictoryPredicate(StrictIdentity), got {result:?}"
    );
}

#[test]
fn compatible_identities_in_all_pass() {
    let event_query = EventQuery::call_global("fetch").unwrap();
    let d = QueryDecl::all(Ok(event_query), []).unwrap();
    assert!(normalize::normalize_query_decl(&d).is_ok());
}

#[test]
fn uncorrelated_all_fails_with_uncorrelated_conjunction() {
    let a = QueryExpr::event(EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        vec![],
    ));
    let b = QueryExpr::event(EventQuery::from_parts_for_test(
        VarId::new(1),
        EventSpec::MemberCall {
            member: SymbolPath::from("doc.createElement"),
        },
        IdentitySpec::Rooted {
            path: SymbolPath::from("doc.createElement"),
        },
        vec![],
    ));
    let all = AllExpr::new(vec![a, b]).unwrap();
    let d = decl(QueryExpr::all(all), 0, "test");
    let result = normalize::normalize_query_decl(&d);
    assert!(
        matches!(
            result,
            Err(crate::api::compiler::validate::QueryCompileError::UncorrelatedConjunction)
        ),
        "expected UncorrelatedConjunction, got {result:?}"
    );
}

#[test]
fn simple_query_has_no_matcher_specific_preparation_requirements() {
    let d = decl(event(0, "fetch"), 0, "fetch");
    let nq = normalize_ok(&d);
    let req = nq.requirements();
    assert!(req.value_resolution().is_empty());
    assert!(!req.flow().local());
    assert!(!req.needs_project_overlay());
}

#[test]
fn constrained_query_has_fact_stream() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap();
    let d = eq.into_query();
    let nq = normalize_ok(&d);
    let req = nq.requirements();
    assert!(
        req.value_resolution()
            .contains(&ValueResolutionRequirement::LocalStaticValues)
    );
}

#[test]
fn module_query_has_project_overlay() {
    let d = EventQuery::call_module("fs", "readFile")
        .unwrap()
        .into_query();
    let nq = normalize_ok(&d);
    let req = nq.requirements();
    assert!(req.needs_project_overlay());
    assert_eq!(
        req.project_requirements(),
        &std::collections::BTreeSet::from([
            ProjectRequirement::ExactModuleExports,
            ProjectRequirement::CallResultIdentities,
        ])
    );
}

#[test]
fn global_query_does_not_need_project_overlay() {
    let d = decl(event(0, "fetch"), 0, "fetch");
    let nq = normalize_ok(&d);
    let req = nq.requirements();
    assert!(!req.needs_project_overlay());
}

#[test]
fn any_merges_requirements_from_branches() {
    let branches = vec![
        EventQuery::call_global("fetch").unwrap().into_query(),
        EventQuery::call_module("fs", "readFile")
            .unwrap()
            .into_query(),
    ];
    let any = QueryDecl::any_with_evidence(branches.into_iter().map(Ok), "test").unwrap();
    let d = any.with_evidence(MatchKind::Call, "test");
    let nq = normalize_ok(&d);
    let req = nq.requirements();
    assert!(
        req.needs_project_overlay(),
        "Any with module branch should need project overlay"
    );
}

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
    let req = nq.requirements();
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
    let req = nq.requirements();
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
