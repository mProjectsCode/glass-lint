use glass_lint_datastructures::SymbolPath;

use crate::api::{
    compiler::{
        error::PhysicalPlanValidationError,
        normalize::normalize_query_decl,
        normalized::{
            ArgumentConstraintGroup, CanonicalArgumentConstraints, EventSlot, NormalizedEmission,
            NormalizedEvent, NormalizedLifecycle, NormalizedRoot,
            ObjectSlot as NormalizedObjectSlot,
        },
        object_flow::CompiledObjectFlow,
        physical::{
            PhysicalPlan, PhysicalRoot, RootBudget, optimize_roots, plan_normalized,
            validate_physical_plan,
        },
        requirements::PlanRequirements,
        rule::{EventSpec, EvidenceDescriptor, IdentityConstraint},
    },
    rule::{
        ArgumentIndex, ArgumentMatcher, EventQuery, MatchKind, QueryDecl, ValueMatcher,
        query::{QueryBuildError, VarId, limits},
    },
};

fn physical_summary(decl: &QueryDecl) -> String {
    let nq = normalize_query_decl(decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();
    plan.summary()
}

fn physical_roots_from_decl(decl: &QueryDecl) -> Vec<PhysicalRoot> {
    let nq = normalize_query_decl(decl).unwrap();
    let plan = plan_normalized(&nq).unwrap();
    plan.roots().to_vec()
}

fn decl(decl: Result<QueryDecl, QueryBuildError>) -> QueryDecl {
    decl.unwrap()
}

#[test]
fn global_call_produces_indexed_scan() {
    let roots = physical_roots_from_decl(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    assert_eq!(roots.len(), 1);
    assert!(
        matches!(&roots[0], PhysicalRoot::IndexedScan { .. }),
        "expected IndexedScan, got {roots:?}"
    );
}

#[test]
fn heuristic_call_produces_indexed_scan() {
    let roots = physical_roots_from_decl(&decl(
        EventQuery::call_heuristic("fetch").map(EventQuery::into_query),
    ));
    assert_eq!(roots.len(), 1);
    assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
}

#[test]
fn constrained_call_produces_constrained_scan() {
    let d = crate::api::rule::EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap()
        .into_query();
    let roots = physical_roots_from_decl(&d);
    assert_eq!(roots.len(), 1);
    assert!(
        matches!(&roots[0], PhysicalRoot::ConstrainedScan { .. }),
        "expected ConstrainedScan, got {roots:?}"
    );
}

#[test]
fn rooted_member_call_produces_indexed_scan() {
    let roots = physical_roots_from_decl(&decl(
        EventQuery::member_call_rooted("document.createElement").map(EventQuery::into_query),
    ));
    assert_eq!(roots.len(), 1);
    assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
}

#[test]
fn returned_subject_produces_returned_scan() {
    let roots = physical_roots_from_decl(&decl(QueryDecl::member_call_returned("create", "send")));
    assert_eq!(roots.len(), 1);
    assert!(
        matches!(&roots[0], PhysicalRoot::ReturnedSubject { .. }),
        "expected ReturnedSubject, got {roots:?}"
    );
}

#[test]
fn instance_subject_produces_instance_scan() {
    let roots = physical_roots_from_decl(&decl(QueryDecl::member_call_instance(
        "pkg", "Client", "send",
    )));
    assert_eq!(roots.len(), 1);
    assert!(
        matches!(&roots[0], PhysicalRoot::InstanceSubject { .. }),
        "expected InstanceSubject, got {roots:?}"
    );
}

#[test]
fn import_exact_produces_indexed_scan() {
    let roots = physical_roots_from_decl(&decl(
        EventQuery::import_exact("node:fs").map(EventQuery::into_query),
    ));
    assert_eq!(roots.len(), 1);
    assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
}

#[test]
fn string_contains_produces_indexed_scan() {
    let roots = physical_roots_from_decl(&decl(
        EventQuery::string_contains("https://").map(EventQuery::into_query),
    ));
    assert_eq!(roots.len(), 1);
    assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
}

#[test]
fn class_reference_produces_indexed_scan() {
    let roots = physical_roots_from_decl(&decl(
        EventQuery::class_heuristic("Worker").map(EventQuery::into_query),
    ));
    assert_eq!(roots.len(), 1);
    assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
}

#[test]
fn constructor_global_produces_indexed_scan() {
    let roots = physical_roots_from_decl(&decl(
        EventQuery::constructor_global("URL").map(EventQuery::into_query),
    ));
    assert_eq!(roots.len(), 1);
    assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
}

#[test]
fn module_call_produces_indexed_scan() {
    let roots = physical_roots_from_decl(&decl(
        EventQuery::call_module("fs", "readFile").map(EventQuery::into_query),
    ));
    assert_eq!(roots.len(), 1);
    assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
}

#[test]
fn member_read_returned_produces_returned_scan() {
    let roots = physical_roots_from_decl(&decl(QueryDecl::member_read_returned("create", "token")));
    assert_eq!(roots.len(), 1);
    assert!(
        matches!(&roots[0], PhysicalRoot::ReturnedSubject { .. }),
        "expected ReturnedSubject, got {roots:?}"
    );
}

#[test]
fn multiple_constraints_on_same_call_fuse_into_one_constrained_scan() {
    let d = crate::api::rule::EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap()
        .with_arg(1, ValueMatcher::static_string().try_equals("/api").unwrap())
        .unwrap()
        .into_query();
    let roots = physical_roots_from_decl(&d);
    assert_eq!(roots.len(), 1);
    match &roots[0] {
        PhysicalRoot::ConstrainedScan { constraints, .. } => {
            let total_predicates: usize = constraints
                .groups()
                .iter()
                .map(|g| g.predicates().len())
                .sum();
            assert_eq!(total_predicates, 2);
        }
        other => panic!("expected ConstrainedScan, got {other:?}"),
    }
}

#[test]
fn alternatives_from_any_produce_multiple_roots() {
    use crate::api::rule::query::{AnyExpr, EventQuery, EventSpec, QueryExpr};
    let branches = vec![
        QueryExpr::event(EventQuery::from_parts_for_test(
            VarId::new(0),
            EventSpec::Call,
            crate::api::rule::query::IdentitySpec::Global {
                name: "fetch".into(),
            },
            vec![],
        )),
        QueryExpr::event(EventQuery::from_parts_for_test(
            VarId::new(1),
            EventSpec::Call,
            crate::api::rule::query::IdentitySpec::Global {
                name: "navigate".into(),
            },
            vec![],
        )),
    ];
    let query = QueryDecl::from_parts_for_test(
        QueryExpr::any(AnyExpr::new(branches).unwrap()),
        crate::api::rule::query::EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "request".into(),
        },
    );
    let nq = normalize_query_decl(&query).unwrap();
    let plan = plan_normalized(&nq).unwrap();
    assert_eq!(plan.roots().len(), 2);
    for root in plan.roots() {
        assert!(
            matches!(root, PhysicalRoot::IndexedScan { .. }),
            "expected IndexedScan for each alternative"
        );
    }
}

#[test]
fn nested_alternatives_respect_the_aggregate_root_budget() {
    use crate::api::rule::query::{AnyExpr, EventQuery, EventSpec, IdentitySpec, QueryExpr};

    fn expression(depth: usize, next: &mut u32) -> QueryExpr {
        if depth == 0 {
            let index = *next;
            *next += 1;
            return QueryExpr::event(EventQuery::from_parts_for_test(
                VarId::new(index),
                EventSpec::Call,
                IdentitySpec::Global {
                    name: format!("root{index}").into(),
                },
                vec![],
            ));
        }
        QueryExpr::any(
            AnyExpr::new(vec![
                expression(depth - 1, next),
                expression(depth - 1, next),
            ])
            .unwrap(),
        )
    }

    let mut next = 0;
    let expression = expression(9, &mut next);
    let query = QueryDecl::from_parts_for_test(
        expression,
        crate::api::rule::query::EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "request".into(),
        },
    );

    let normalized = normalize_query_decl(&query).unwrap();
    assert!(matches!(
        plan_normalized(&normalized),
        Err(PhysicalPlanValidationError::TooManyRoots(_))
    ));
}

#[test]
fn root_budget_rejects_the_first_root_over_the_limit() {
    let mut budget = RootBudget::new();
    for _ in 0..crate::api::compiler::limits::MAX_PHYSICAL_ROOTS_PER_RULE {
        budget.reserve().unwrap();
    }
    assert!(matches!(
        budget.reserve(),
        Err(PhysicalPlanValidationError::TooManyRoots(_))
    ));
}

#[test]
fn physical_plan_rejects_empty_roots() {
    assert!(matches!(
        PhysicalPlan::from_planned_roots(Box::new([])),
        Err(PhysicalPlanValidationError::EmptyRoots)
    ));
}

#[test]
fn plan_summary_counts_roots() {
    let summary = physical_summary(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    assert_eq!(
        summary,
        "roots=1 indexed_scans=1 constrained_scans=0 returned_subjects=0 instance_subjects=0 lifecycle_plans=0 local_flow=no cross_call_flow=no project_overlay=no project_requirements={}"
    );
}

#[test]
fn plan_explanation_is_deterministic_and_names_operator_choice() {
    let first = physical_summary(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    let query = decl(EventQuery::call_global("fetch").map(EventQuery::into_query));
    let normalized = normalize_query_decl(&query).unwrap();
    let plan = plan_normalized(&normalized).unwrap();
    assert!(plan.explain().contains("indexed event=Call"));
    assert!(plan.explain().contains("optimization canonical-root-order"));
    assert_eq!(first, plan.summary());
    assert_eq!(plan.explain(), plan.explain());
}

#[test]
fn optimizer_deduplicates_only_identical_evidence_bearing_roots() {
    let query = decl(EventQuery::call_global("fetch").map(EventQuery::into_query));
    let normalized = normalize_query_decl(&query).unwrap();
    let root = plan_normalized(&normalized).unwrap().roots()[0].clone();
    assert_eq!(optimize_roots(vec![root.clone(), root]).len(), 1);

    let first = decl(EventQuery::call_global("fetch").map(EventQuery::into_query));
    let second = decl(EventQuery::call_global("fetch").map(EventQuery::into_query))
        .with_evidence(MatchKind::CallArgument, "fetch-argument");
    let first = plan_normalized(&normalize_query_decl(&first).unwrap())
        .unwrap()
        .roots()[0]
        .clone();
    let second = plan_normalized(&normalize_query_decl(&second).unwrap())
        .unwrap()
        .roots()[0]
        .clone();
    assert_eq!(optimize_roots(vec![first, second]).len(), 2);
}

#[test]
fn plan_summary_shows_constrained_scan() {
    let d = crate::api::rule::EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap()
        .into_query();
    let summary = physical_summary(&d);
    assert_eq!(
        summary,
        "roots=1 indexed_scans=0 constrained_scans=1 returned_subjects=0 instance_subjects=0 lifecycle_plans=0 local_flow=no cross_call_flow=no project_overlay=no project_requirements={}"
    );
    assert!(summary.contains("indexed_scans=0"), "summary: {summary}");
}

#[test]
fn plan_summary_shows_project_overlay_for_module_queries() {
    let summary = physical_summary(&decl(
        EventQuery::call_module("fs", "readFile").map(EventQuery::into_query),
    ));
    assert!(
        summary.contains("project_overlay=yes"),
        "summary: {summary}"
    );
}

#[test]
fn plan_summary_shows_no_project_overlay_for_global_queries() {
    let summary = physical_summary(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    assert!(summary.contains("project_overlay=no"), "summary: {summary}");
}

#[test]
fn empty_identity_fails_validation() {
    let roots = Box::new([PhysicalRoot::IndexedScan {
        identity: IdentityConstraint::Global { name: "".into() },
        event: EventSpec::Call,
        evidence: EvidenceDescriptor {
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    }]);
    assert_eq!(
        PhysicalPlan::try_new(roots.clone(), &PlanRequirements::default()),
        Err(PhysicalPlanValidationError::ImpossibleDimensions)
    );
    let plan = PhysicalPlan::new(roots, PlanRequirements::default());
    assert_eq!(
        validate_physical_plan(&plan),
        Err(PhysicalPlanValidationError::ImpossibleDimensions)
    );
}

#[test]
fn object_slot_sentinel_is_rejected_by_relation_constructor() {
    assert_eq!(
        PhysicalRoot::returned_subject(
            IdentityConstraint::Rooted {
                path: "document.create".into(),
            },
            NormalizedObjectSlot::from_raw(u32::MAX),
            SymbolPath::from("send"),
            EventSpec::MemberCall {
                member: SymbolPath::from("send"),
            },
            EvidenceDescriptor {
                kind: MatchKind::Call,
                symbol: "send".into(),
            },
        ),
        Err(PhysicalPlanValidationError::ImpossibleDimensions)
    );
}

#[test]
fn valid_roots_pass_validation() {
    let roots = Box::new([PhysicalRoot::IndexedScan {
        identity: IdentityConstraint::Global {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        evidence: EvidenceDescriptor {
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    }]);
    let plan = PhysicalPlan::new(roots, PlanRequirements::default());
    assert!(validate_physical_plan(&plan).is_ok());
}

#[test]
fn requirements_must_match_executable_roots() {
    let roots = Box::new([PhysicalRoot::IndexedScan {
        identity: IdentityConstraint::Global {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        evidence: EvidenceDescriptor {
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    }]);
    let mut requirements = PlanRequirements::default();
    requirements.require_local_flow();
    let plan = PhysicalPlan::new(roots, requirements);
    assert_eq!(
        validate_physical_plan(&plan),
        Err(PhysicalPlanValidationError::RequirementsMismatch)
    );
}

#[path = "physical_extended.rs"]
mod physical_extended;
