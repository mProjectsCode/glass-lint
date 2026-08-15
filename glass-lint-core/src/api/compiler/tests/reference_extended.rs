use super::*;

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
    assert_eq!(logical_witnesses(&nq, &[incomplete]).len(), 0);
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
