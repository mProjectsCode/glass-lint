use std::collections::BTreeMap;

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    compiler::{
        normalize::normalize_query_decl,
        normalized::NormalizedQuery,
        physical::plan_normalized,
        reference::{
            ReferenceCertainty, ReferenceCompleteness, ReferenceRow, ReferenceSupport,
            ReferenceSupportKind, ReferenceValue, ReferenceWitness, evaluate_supported_logical,
            evaluate_supported_physical,
        },
    },
    rule::{
        ArgumentIndex, ValueMatcher,
        query::{
            EmissionDecl, EventQuery, EventSpec, IdentitySpec, LifecycleQuery, QueryDecl,
            QueryExpr,
            lifecycle::{LifecycleCompletion, LifecycleCondition, LifecycleEvent, LifecycleSink},
        },
    },
};

fn row(event: u32, event_kind: EventSpec, identity: IdentitySpec, path: u32) -> ReferenceRow {
    ReferenceRow {
        event,
        event_kind,
        identity,
        arguments: BTreeMap::new(),
        object: None,
        support: None,
        path,
        completeness: ReferenceCompleteness::Complete,
    }
}

fn row_with_args(
    event: u32,
    event_kind: EventSpec,
    identity: IdentitySpec,
    path: u32,
    arguments: BTreeMap<ArgumentIndex, ReferenceValue>,
) -> ReferenceRow {
    ReferenceRow {
        event,
        event_kind,
        identity,
        arguments,
        object: None,
        support: None,
        path,
        completeness: ReferenceCompleteness::Complete,
    }
}

fn row_unknown(
    event: u32,
    event_kind: EventSpec,
    identity: IdentitySpec,
    path: u32,
) -> ReferenceRow {
    ReferenceRow {
        event,
        event_kind,
        identity,
        arguments: BTreeMap::new(),
        object: None,
        support: None,
        path,
        completeness: ReferenceCompleteness::Unknown,
    }
}

fn logical_witnesses(query: &NormalizedQuery, rows: &[ReferenceRow]) -> Vec<ReferenceWitness> {
    evaluate_supported_logical(query, rows)
}

fn physical_witnesses(
    plan: &crate::api::compiler::physical::PhysicalPlan,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    evaluate_supported_physical(plan, rows)
}

fn witnesses_equal(
    query: &NormalizedQuery,
    plan: &crate::api::compiler::physical::PhysicalPlan,
    rows: &[ReferenceRow],
) -> bool {
    let logical = logical_witnesses(query, rows);
    let physical = physical_witnesses(plan, rows);
    logical == physical
}

#[test]
fn empty_rows_produce_no_witnesses() {
    let decl = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();
    assert!(witnesses_equal(&nq, &plan, &[]));
    assert!(logical_witnesses(&nq, &[]).is_empty());
    assert!(physical_witnesses(&plan, &[]).is_empty());
}

#[test]
fn matching_rows_produce_witnesses() {
    let decl = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();

    let rows = vec![row(
        1,
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        0,
    )];
    assert!(witnesses_equal(&nq, &plan, &rows));
    assert_eq!(logical_witnesses(&nq, &rows).len(), 1);
}

#[test]
fn non_matching_rows_produce_no_witnesses() {
    let decl = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();

    let rows = vec![row(
        1,
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("navigate"),
        },
        0,
    )];
    assert!(witnesses_equal(&nq, &plan, &rows));
    assert!(logical_witnesses(&nq, &rows).is_empty());
}

#[test]
fn duplicate_rows_produce_deduplicated_witnesses() {
    let decl = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();

    let rows = vec![
        row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
        ),
        row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
        ),
    ];
    assert!(witnesses_equal(&nq, &plan, &rows));
    assert_eq!(logical_witnesses(&nq, &rows).len(), 1);
}

#[test]
fn any_branch_order_produces_same_witnesses() {
    let decl_a = QueryDecl::any_with_evidence(
        [
            Ok(EventQuery::call_global("fetch").unwrap().into_query()),
            Ok(EventQuery::call_global("navigate").unwrap().into_query()),
        ],
        "call",
    )
    .unwrap();
    let decl_b = QueryDecl::any_with_evidence(
        [
            Ok(EventQuery::call_global("navigate").unwrap().into_query()),
            Ok(EventQuery::call_global("fetch").unwrap().into_query()),
        ],
        "call",
    )
    .unwrap();

    let nq_a = normalize_query_decl(&decl_a).unwrap();
    let nq_b = normalize_query_decl(&decl_b).unwrap();
    let plan_a = plan_normalized(&nq_a).unwrap();
    let plan_b = plan_normalized(&nq_b).unwrap();

    let rows = vec![
        row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
        ),
        row(
            2,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("navigate"),
            },
            1,
        ),
    ];

    let l_a = logical_witnesses(&nq_a, &rows);
    let l_b = logical_witnesses(&nq_b, &rows);
    let p_a = physical_witnesses(&plan_a, &rows);
    let p_b = physical_witnesses(&plan_b, &rows);

    assert_eq!(l_a, l_b, "logical witnesses should be order-independent");
    assert_eq!(p_a, p_b, "physical witnesses should be order-independent");
}

#[test]
fn unknown_row_produces_possible_witness() {
    let decl = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();

    let rows = vec![row_unknown(
        1,
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        0,
    )];
    assert!(witnesses_equal(&nq, &plan, &rows));
    let witnesses = logical_witnesses(&nq, &rows);
    assert_eq!(witnesses.len(), 1);
    assert_eq!(witnesses[0].certainty, ReferenceCertainty::Possible);
}

#[test]
fn complete_row_produces_definite_witness() {
    let decl = EventQuery::call_global("fetch").unwrap().into_query();
    let nq = normalize_query_decl(&decl).unwrap();

    let rows = vec![row(
        1,
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        0,
    )];
    let witnesses = logical_witnesses(&nq, &rows);
    assert_eq!(witnesses[0].certainty, ReferenceCertainty::Definite);
}

#[test]
fn unknown_alternative_does_not_erase_complete_witness() {
    let decl = QueryDecl::any_with_evidence(
        [
            Ok(EventQuery::call_global("fetch").unwrap().into_query()),
            Ok(EventQuery::call_global("navigate").unwrap().into_query()),
        ],
        "call",
    )
    .unwrap();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();

    let rows = vec![row(
        1,
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        0,
    )];
    assert!(witnesses_equal(&nq, &plan, &rows));
    let witnesses = logical_witnesses(&nq, &rows);
    assert_eq!(witnesses.len(), 1);
    assert_eq!(witnesses[0].certainty, ReferenceCertainty::Definite);
}

#[test]
fn witnesses_are_sorted_deterministically() {
    let decl = QueryDecl::any_with_evidence(
        [
            Ok(EventQuery::call_global("navigate").unwrap().into_query()),
            Ok(EventQuery::call_global("fetch").unwrap().into_query()),
        ],
        "call",
    )
    .unwrap();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();

    let rows = vec![
        row(
            2,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("navigate"),
            },
            1,
        ),
        row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
        ),
    ];

    let l = logical_witnesses(&nq, &rows);
    let p = physical_witnesses(&plan, &rows);
    assert_eq!(l, p);

    for pair in l.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "witnesses must be sorted: {:?} >= {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn constrained_scan_matches_arguments() {
    let eq = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().try_equals("/api").unwrap())
        .unwrap();
    let decl = eq.into_query();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();

    let mut args = BTreeMap::new();
    args.insert(
        ArgumentIndex::new_unchecked(0),
        ReferenceValue::StaticString("/api".into()),
    );

    let matching = vec![row_with_args(
        1,
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        0,
        args.clone(),
    )];
    assert!(witnesses_equal(&nq, &plan, &matching));
    assert_eq!(logical_witnesses(&nq, &matching).len(), 1);

    let mut wrong_args = BTreeMap::new();
    wrong_args.insert(
        ArgumentIndex::new_unchecked(0),
        ReferenceValue::StaticString("/other".into()),
    );
    let non_matching = vec![row_with_args(
        1,
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        0,
        wrong_args,
    )];
    assert!(witnesses_equal(&nq, &plan, &non_matching));
    assert!(logical_witnesses(&nq, &non_matching).is_empty());
}

#[test]
fn argument_filter_order_produces_same_witnesses() {
    let eq_a = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().try_equals("/api").unwrap())
        .unwrap()
        .with_arg(1, ValueMatcher::static_string().try_equals("post").unwrap())
        .unwrap();
    let eq_b = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(1, ValueMatcher::static_string().try_equals("post").unwrap())
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().try_equals("/api").unwrap())
        .unwrap();

    let decl_a = eq_a.into_query();
    let decl_b = eq_b.into_query();

    let nq_a = normalize_query_decl(&decl_a).unwrap();
    let nq_b = normalize_query_decl(&decl_b).unwrap();
    let plan_a = plan_normalized(&nq_a).unwrap();
    let plan_b = plan_normalized(&nq_b).unwrap();

    let mut args = BTreeMap::new();
    args.insert(
        ArgumentIndex::new_unchecked(0),
        ReferenceValue::StaticString("/api".into()),
    );
    args.insert(
        ArgumentIndex::new_unchecked(1),
        ReferenceValue::StaticString("post".into()),
    );

    let rows = vec![row_with_args(
        1,
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        0,
        args,
    )];

    let l_a = logical_witnesses(&nq_a, &rows);
    let l_b = logical_witnesses(&nq_b, &rows);
    let p_a = physical_witnesses(&plan_a, &rows);
    let p_b = physical_witnesses(&plan_b, &rows);

    assert_eq!(
        l_a, l_b,
        "logical witnesses should be filter-order independent"
    );
    assert_eq!(
        p_a, p_b,
        "physical witnesses should be filter-order independent"
    );
}

#[test]
fn returned_subject_produces_support_evidence() {
    let decl = QueryDecl::member_call_returned("create", "send").unwrap();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();

    let rows = vec![ReferenceRow {
        event: 42,
        event_kind: EventSpec::MemberCall {
            member: SymbolPath::from("send"),
        },
        identity: IdentitySpec::Rooted {
            path: SymbolPath::from("create"),
        },
        arguments: BTreeMap::new(),
        object: Some(7),
        support: Some(ReferenceSupport {
            event: 7,
            path: 0,
            kind: ReferenceSupportKind::Producer,
        }),
        path: 0,
        completeness: ReferenceCompleteness::Complete,
    }];

    assert!(witnesses_equal(&nq, &plan, &rows));
    let w = logical_witnesses(&nq, &rows);
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].support_events, vec![7]);

    let mut incomplete = rows[0].clone();
    incomplete.support = None;
    assert!(logical_witnesses(&nq, &[incomplete]).is_empty());
}

#[test]
fn different_path_keys_produce_separate_witnesses() {
    let decl = QueryDecl::any_with_evidence(
        [
            Ok(EventQuery::call_global("fetch").unwrap().into_query()),
            Ok(EventQuery::call_global("navigate").unwrap().into_query()),
        ],
        "call",
    )
    .unwrap();
    let nq = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();

    let rows = vec![
        row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            10,
        ),
        row(
            2,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("navigate"),
            },
            20,
        ),
    ];

    let l = logical_witnesses(&nq, &rows);
    let p = physical_witnesses(&plan, &rows);
    assert_eq!(l, p);
    assert_eq!(l.len(), 2);
    assert!(l.iter().any(|w| w.path_key == 10));
    assert!(l.iter().any(|w| w.path_key == 20));
}

#[test]
fn lifecycle_reference_matches_logical_and_physical_plans() {
    let lifecycle = LifecycleQuery::catalog_builder("resource")
        .source(EventQuery::call_global("open").unwrap())
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "ready",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::any_sink([
            LifecycleSink::argument_of_global("consume", 0).unwrap(),
        ]))
        .build()
        .unwrap();
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::lifecycle(lifecycle),
        EmissionDecl {
            primary_var: crate::api::rule::query::VarId::new(0),
            kind: MatchKind::Call,
            symbol: "resource".into(),
        },
    );
    let normalized = normalize_query_decl(&decl).unwrap();
    let plan = plan_normalized(&normalized).unwrap();
    let mut condition_args = BTreeMap::new();
    condition_args.insert(
        ArgumentIndex::new_unchecked(0),
        ReferenceValue::StaticString("ready".into()),
    );
    let mut sink_args = BTreeMap::new();
    sink_args.insert(
        ArgumentIndex::new_unchecked(0),
        ReferenceValue::StaticString("resource".into()),
    );
    let rows = vec![
        row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("open"),
            },
            0,
        ),
        row_with_args(
            2,
            EventSpec::PropertyWrite {
                property: SymbolPath::from("ready"),
            },
            IdentitySpec::Global {
                name: SmolStr::new("open"),
            },
            0,
            condition_args,
        ),
        row_with_args(
            3,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("consume"),
            },
            0,
            sink_args,
        ),
    ];
    assert_eq!(
        logical_witnesses(&normalized, &rows),
        physical_witnesses(&plan, &rows)
    );
    assert_eq!(logical_witnesses(&normalized, &rows).len(), 1);
    assert_eq!(logical_witnesses(&normalized, &rows)[0].primary_event, 3);
}
