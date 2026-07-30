use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    compiler::{
        normalize::{self},
        normalized::{NormalizedQuery, NormalizedRoot},
        requirements::{PlanRequirements, ProjectRequirement, ValueResolutionRequirement},
    },
    rule::{
        ValueMatcher,
        query::{
            AllExpr, AnyExpr, EmissionDecl, EventQuery, EventRequirement, EventSpec, IdentitySpec,
            LifecycleQuery, QueryDecl, QueryExpr, VarId,
        },
    },
};

fn event(var: u32, name: &str) -> QueryExpr {
    QueryExpr::event(EventQuery {
        var: VarId::new(var),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new(name),
        },
        constraints: vec![],
    })
}

fn decl(expr: QueryExpr, primary_var: u32, symbol: &str) -> QueryDecl {
    QueryDecl {
        expression: expr,
        emission: EmissionDecl {
            primary_var: VarId::new(primary_var),
            kind: MatchKind::Call,
            symbol: symbol.into(),
        },
    }
}

fn normalize_ok(decl: &QueryDecl) -> NormalizedQuery {
    normalize::normalize_query_decl(decl).unwrap()
}

#[test]
fn simple_event_normalizes_to_event_root() {
    let d = decl(event(0, "fetch"), 0, "fetch");
    let nq = normalize_ok(&d);
    assert!(matches!(nq.root(), NormalizedRoot::Event(_)));
}

#[test]
fn lifecycle_normalizes_to_lifecycle_root() {
    let source = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("document.createElement"),
        },
        identity: IdentitySpec::Rooted {
            path: SymbolPath::from("document.createElement"),
        },
        constraints: vec![],
    };
    let lc = LifecycleQuery::new(
        "remote-script",
        vec![source],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("src", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
    )
    .unwrap();
    let d = QueryDecl {
        expression: QueryExpr::lifecycle(lc),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::CallArgument,
            symbol: "remote-script".into(),
        },
    };
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
        Ok(EventRequirement::argument(1, ValueMatcher::static_string().equals("/api")).unwrap()),
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
    let a = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    });
    let b = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Construct,
        identity: IdentitySpec::Global {
            name: SmolStr::new("URL"),
        },
        constraints: vec![],
    });
    let all = AllExpr::new(vec![a, b]).unwrap();
    let d = decl(QueryExpr::all(all), 0, "test");
    let result = normalize::normalize_query_decl(&d);
    assert!(
        matches!(
            result,
            Err(
                crate::api::compiler::validate::QueryCompileError::ContradictoryPredicate {
                    detail: crate::api::compiler::validate::ContradictionKind::EventKind,
                    ..
                }
            )
        ),
        "expected ContradictoryPredicate(EventKind), got {result:?}"
    );
}

#[test]
fn incompatible_identities_in_all_produce_contradiction() {
    let a = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    });
    let b = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("navigate"),
        },
        constraints: vec![],
    });
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
    let a = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    });
    let b = QueryExpr::event(EventQuery {
        var: VarId::new(1),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("doc.createElement"),
        },
        identity: IdentitySpec::Rooted {
            path: SymbolPath::from("doc.createElement"),
        },
        constraints: vec![],
    });
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
    assert!(!req.flow().local);
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
    let any = QueryDecl::any(branches.into_iter().map(Ok)).unwrap();
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
    let source = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("document.createElement"),
        },
        identity: IdentitySpec::Rooted {
            path: SymbolPath::from("document.createElement"),
        },
        constraints: vec![],
    };
    let lc = LifecycleQuery::new(
        "test",
        vec![source],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("src", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
    )
    .unwrap();
    let d = QueryDecl {
        expression: QueryExpr::lifecycle(lc),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::CallArgument,
            symbol: "test".into(),
        },
    };
    let nq = normalize_ok(&d);
    let req = nq.requirements();
    assert!(req.flow().local, "lifecycle should need local flow");
    assert!(
        req.flow().cross_call,
        "lifecycle should need cross-call flow"
    );
}

#[test]
fn global_query_has_only_calls_requirement() {
    let d = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize_ok(&d);
    let req = nq.requirements();
    assert!(req.value_resolution().is_empty());
    assert!(!req.flow().local);
    assert!(!req.flow().cross_call);
    assert!(!req.needs_project_overlay());
}

#[test]
fn lifecycle_is_not_flattened_or_sorted() {
    let source = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("document.createElement"),
        },
        identity: IdentitySpec::Rooted {
            path: SymbolPath::from("document.createElement"),
        },
        constraints: vec![],
    };
    let lc = LifecycleQuery::new(
        "test",
        vec![source],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
    )
    .unwrap();
    let d = QueryDecl {
        expression: QueryExpr::lifecycle(lc),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::CallArgument,
            symbol: "test".into(),
        },
    };
    let nq = normalize_ok(&d);
    assert!(matches!(nq.root(), NormalizedRoot::Lifecycle(_)));
}

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

#[test]
fn normalized_query_compiles_through_full_pipeline() {
    let d = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize::normalize_query_decl(&d).unwrap();
    let _plan = crate::api::compiler::physical::plan_normalized(&nq);
}

#[test]
fn duplicate_filters_do_not_duplicate_work_or_evidence() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().equals("/api"))
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().equals("/api"))
        .unwrap();
    let nq = normalize::normalize_query_decl(&eq.into_query()).unwrap();
    match nq.root() {
        NormalizedRoot::Event(ev) => {
            assert_eq!(
                ev.arguments.to_flat_vec().len(),
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
        .with_arg(0, ValueMatcher::static_string().equals("/api"))
        .unwrap();
    let req = EventRequirement::argument(0, ValueMatcher::static_string().equals("/api")).unwrap();
    let d = QueryDecl::all(Ok(eq), [Ok(req)]).unwrap();
    let nq = normalize::normalize_query_decl(&d).unwrap();
    match nq.root() {
        NormalizedRoot::Event(ev) => {
            assert_eq!(
                ev.arguments.to_flat_vec().len(),
                1,
                "duplicate constraints from All branches must be deduplicated"
            );
        }
        other => panic!("expected Event, got {other:?}"),
    }
}

#[test]
fn distinct_lifecycle_conditions_never_compare_as_same_ordering_key() {
    let source_a = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("document.createElement"),
        },
        identity: IdentitySpec::Rooted {
            path: SymbolPath::from("document.createElement"),
        },
        constraints: vec![],
    };
    let source_b = EventQuery {
        var: VarId::new(1),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("doc.createElement"),
        },
        identity: IdentitySpec::Rooted {
            path: SymbolPath::from("doc.createElement"),
        },
        constraints: vec![],
    };

    let lc_a = LifecycleQuery::new(
        "test-a",
        vec![source_a],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("src", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
    )
    .unwrap();

    let lc_b = LifecycleQuery::new(
        "test-b",
        vec![source_b],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("href", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
    )
    .unwrap();

    let d_a = QueryDecl {
        expression: QueryExpr::lifecycle(lc_a),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::CallArgument,
            symbol: "test-a".into(),
        },
    };
    let d_b = QueryDecl {
        expression: QueryExpr::lifecycle(lc_b),
        emission: EmissionDecl {
            primary_var: VarId::new(1),
            kind: MatchKind::CallArgument,
            symbol: "test-b".into(),
        },
    };

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
            let flat = ev.arguments.to_flat_vec();
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
