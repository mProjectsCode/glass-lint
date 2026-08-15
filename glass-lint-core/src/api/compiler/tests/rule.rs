use glass_lint_datastructures::SymbolPath;

use crate::api::{
    compiler::{
        physical,
        rule::{CompiledMatcherPlan, EventSpec, IdentityConstraint},
    },
    rule::{EventQuery, MatchKind, QueryDecl, ValueMatcher},
};

#[test]
fn every_declaration_compiles_into_one_plan() {
    let queries = vec![
        EventQuery::call_global("fetch").unwrap().into_query(),
        EventQuery::member_call_rooted("window.open")
            .unwrap()
            .into_query(),
        EventQuery::member_read_rooted("window.location")
            .unwrap()
            .into_query(),
        EventQuery::import_exact("node:fs").unwrap().into_query(),
        EventQuery::import_package("@scope/pkg")
            .unwrap()
            .into_query(),
        EventQuery::string_contains("https://")
            .unwrap()
            .into_query(),
        EventQuery::class_heuristic("Worker").unwrap().into_query(),
        EventQuery::constructor_global("URL").unwrap().into_query(),
        QueryDecl::member_call_returned("create", "send").unwrap(),
        QueryDecl::member_read_returned("create", "token").unwrap(),
        QueryDecl::member_call_instance("pkg", "Client", "send").unwrap(),
    ];
    let plan = CompiledMatcherPlan::compile(&queries).unwrap();
    assert_ne!(plan.physical_roots().len(), 0);
    assert!(plan.plan_explanation().starts_with("plan roots="));
}

#[test]
fn argument_matcher_compiles_to_constrained_scan() {
    let query = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap()
        .into_query()
        .with_evidence(MatchKind::CallArgument, "fetch");
    let plan = CompiledMatcherPlan::compile(&[query]).unwrap();
    let roots = plan.physical_roots();
    assert_eq!(roots.len(), 1);
    match &roots[0] {
        physical::PhysicalRoot::ConstrainedScan {
            constraints,
            evidence,
            ..
        } => {
            assert_ne!(constraints.groups().len(), 0);
            assert_eq!(evidence.kind, MatchKind::CallArgument);
        }
        other => panic!("expected ConstrainedScan, got {other:?}"),
    }
}

#[test]
fn equivalent_declarations_compile_to_identical_queries() {
    let first = vec![
        EventQuery::call_global("fetch").unwrap().into_query(),
        EventQuery::member_read_rooted("location.href")
            .unwrap()
            .into_query(),
    ];
    let second = vec![
        EventQuery::member_read_rooted("location.href")
            .unwrap()
            .into_query(),
        EventQuery::call_global("fetch").unwrap().into_query(),
    ];

    let first = CompiledMatcherPlan::compile(&first).unwrap();
    let second = CompiledMatcherPlan::compile(&second).unwrap();
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
}

#[test]
fn query_plan_compiles_declarations_into_physical_roots() {
    let roots = {
        let queries = vec![
            EventQuery::call_global("fetch").unwrap().into_query(),
            EventQuery::member_call_rooted("window.open")
                .unwrap()
                .into_query(),
            QueryDecl::member_read_returned("create", "token").unwrap(),
            QueryDecl::member_call_instance("pkg", "Client", "send").unwrap(),
            EventQuery::import_exact("node:fs").unwrap().into_query(),
            EventQuery::string_contains("https://")
                .unwrap()
                .into_query(),
        ];
        let plan = CompiledMatcherPlan::compile(&queries).unwrap();
        plan.physical_roots().to_vec()
    };
    assert!(roots.iter().any(|root| matches!(
        root,
        physical::PhysicalRoot::IndexedScan {
            identity: IdentityConstraint::Global { name },
            event: EventSpec::Call, ..
        } if name == "fetch"
    )));
    assert!(roots.iter().any(|root| matches!(
        root,
        physical::PhysicalRoot::IndexedScan {
            identity: IdentityConstraint::Rooted { path },
            event: EventSpec::MemberCall { member }, ..
        } if *path == SymbolPath::from("window.open") && member.eq_chain("window.open")
    )));
    assert!(roots.iter().any(|root| matches!(
        root,
        physical::PhysicalRoot::ReturnedSubject {
            producer: IdentityConstraint::Rooted { path },
            event: EventSpec::MemberRead { member }, ..
        } if path.eq_chain("create") && member.eq_chain("token")
    )));
    assert!(roots.iter().any(|root| matches!(
        root,
        physical::PhysicalRoot::InstanceSubject {
            constructor: IdentityConstraint::ModuleExport { module, export },
            member, ..
        } if module == "pkg" && export == "Client" && member.eq_chain("send")
    )));
    assert!(roots.iter().any(|root| matches!(
        root,
        physical::PhysicalRoot::IndexedScan {
            event: EventSpec::Import,
            ..
        }
    )));
    assert!(roots.iter().any(|root| matches!(
        root,
        physical::PhysicalRoot::IndexedScan {
            event: EventSpec::StringReference,
            ..
        }
    )));
}

#[test]
fn query_plan_normalization_is_idempotent_and_order_independent() {
    let first = vec![
        EventQuery::call_heuristic("fetch").unwrap().into_query(),
        EventQuery::member_read_rooted("location.href")
            .unwrap()
            .into_query(),
    ];
    let second = vec![
        EventQuery::member_read_rooted("location.href")
            .unwrap()
            .into_query(),
        EventQuery::call_heuristic("fetch").unwrap().into_query(),
    ];
    let first = CompiledMatcherPlan::compile(&first).unwrap();
    let second = CompiledMatcherPlan::compile(&second).unwrap();
    assert_eq!(
        format!("{:?}", first.physical_roots()),
        format!("{:?}", second.physical_roots())
    );
}

#[test]
fn decl_with_argument_constraint_keeps_call_kind() {
    let query = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap()
        .into_query()
        .with_evidence(MatchKind::CallArgument, "fetch");
    let plan = CompiledMatcherPlan::compile(&[query]).unwrap();
    let roots = plan.physical_roots();
    assert_eq!(roots.len(), 1);
    assert!(matches!(
        &roots[0],
        physical::PhysicalRoot::ConstrainedScan { .. }
    ));
}
