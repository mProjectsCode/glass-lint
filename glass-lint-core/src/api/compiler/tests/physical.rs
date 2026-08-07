use crate::api::{
    classification::MatchKind,
    compiler::{
        error::PhysicalPlanValidationError,
        normalize::normalize_query_decl,
        normalized::{ArgumentConstraintGroup, CanonicalArgumentConstraints},
        object_flow::CompiledObjectFlow,
        physical::{
            PhysicalPlan, PhysicalRoot, optimize_roots, plan_normalized, validate_physical_plan,
        },
        requirements::PlanRequirements,
        rule::{EventPredicate, EvidenceDescriptor, IdentityConstraint},
    },
    rule::{
        ArgumentIndex, ArgumentMatcher, EventQuery, QueryDecl, ValueMatcher,
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
fn plan_summary_counts_roots() {
    let summary = physical_summary(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    assert_eq!(
        summary,
        "roots=1 indexed_scans=1 constrained_scans=0 returned_subjects=0 instance_subjects=0 lifecycle_plans=0 local_flow=no cross_call_flow=no project_overlay=no value_resolution={} project_requirements={}"
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
        "roots=1 indexed_scans=0 constrained_scans=1 returned_subjects=0 instance_subjects=0 lifecycle_plans=0 local_flow=no cross_call_flow=no project_overlay=no value_resolution={LocalStaticValues} project_requirements={}"
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
        identity: IdentityConstraint::Global {
            name: "".into(),
            strength: crate::api::compiler::rule::IdentityStrength::Strict,
        },
        event: EventPredicate::Call,
        evidence: EvidenceDescriptor {
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    }]);
    assert_eq!(
        PhysicalPlan::try_new(roots.clone(), PlanRequirements::default()),
        Err(PhysicalPlanValidationError::ImpossibleDimensions)
    );
    let plan = PhysicalPlan::new(roots, PlanRequirements::default());
    assert_eq!(
        validate_physical_plan(&plan),
        Err(PhysicalPlanValidationError::ImpossibleDimensions)
    );
}

#[test]
fn valid_roots_pass_validation() {
    let roots = Box::new([PhysicalRoot::IndexedScan {
        identity: IdentityConstraint::Global {
            name: "fetch".into(),
            strength: crate::api::compiler::rule::IdentityStrength::Strict,
        },
        event: EventPredicate::Call,
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
            strength: crate::api::compiler::rule::IdentityStrength::Strict,
        },
        event: EventPredicate::Call,
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

#[test]
fn lifecycle_evidence_bound_is_validated_at_the_physical_boundary() {
    let flow = CompiledObjectFlow::test_with_evidence_counts(
        limits::MAX_LIFECYCLE_EVENTS + 1,
        limits::MAX_LIFECYCLE_SINKS,
    );
    let roots = Box::new([PhysicalRoot::Lifecycle { flow }]);
    let plan = PhysicalPlan::new(roots, PlanRequirements::default());

    assert_eq!(
        validate_physical_plan(&plan),
        Err(PhysicalPlanValidationError::ExcessiveLifecycleEvidence {
            requirements: limits::MAX_LIFECYCLE_EVENTS + 1,
            sinks: limits::MAX_LIFECYCLE_SINKS,
        })
    );
}

#[test]
fn equivalent_declarations_produce_identical_plans() {
    let roots1 = physical_roots_from_decl(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    let roots2 = physical_roots_from_decl(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    assert_eq!(roots1, roots2);
}

#[test]
fn different_declarations_produce_different_plans() {
    let roots1 = physical_roots_from_decl(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    let roots2 = physical_roots_from_decl(&decl(
        EventQuery::call_global("navigate").map(EventQuery::into_query),
    ));
    assert_ne!(roots1, roots2);
}

#[test]
fn plan_summary_is_stable_across_equal_queries() {
    let s1 = physical_summary(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    let s2 = physical_summary(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    assert_eq!(s1, s2);
}

#[test]
fn plan_summary_shows_no_flow_for_global_query() {
    let summary = physical_summary(&decl(
        EventQuery::call_global("fetch").map(EventQuery::into_query),
    ));
    assert!(summary.contains("local_flow=no"), "summary: {summary}");
    assert!(summary.contains("cross_call_flow=no"), "summary: {summary}");
}

#[test]
fn plan_summary_shows_no_flow_for_module_query() {
    let summary = physical_summary(&decl(
        EventQuery::call_module("fs", "readFile").map(EventQuery::into_query),
    ));
    assert!(summary.contains("local_flow=no"), "summary: {summary}");
    assert!(summary.contains("cross_call_flow=no"), "summary: {summary}");
    assert!(
        summary.contains("project_overlay=yes"),
        "summary: {summary}"
    );
}

#[test]
#[allow(clippy::cast_possible_truncation)]
fn excessive_groups_fails_validation() {
    let groups: Vec<ArgumentConstraintGroup> = (0..=limits::MAX_ARGUMENT_GROUPS)
        .map(|i| ArgumentConstraintGroup {
            index: ArgumentIndex::new_unchecked(i as u8),
            predicates: Box::new([ArgumentMatcher::from(ValueMatcher::static_string())]),
        })
        .collect();
    let constraints = CanonicalArgumentConstraints {
        groups: groups.into_boxed_slice(),
    };
    let plan = PhysicalPlan::new(
        Box::new([PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Global {
                name: "fetch".into(),
                strength: crate::api::compiler::rule::IdentityStrength::Strict,
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        }]),
        PlanRequirements::default(),
    );
    assert!(
        matches!(
            validate_physical_plan(&plan),
            Err(PhysicalPlanValidationError::ExcessiveArgumentGroups(_))
        ),
        "expected ExcessiveArgumentGroups error"
    );
}

#[test]
fn excessive_predicate_count_fails_validation() {
    let predicates: Vec<ArgumentMatcher> = (0..=limits::MAX_PREDICATES_PER_ARGUMENT)
        .map(|_| ArgumentMatcher::from(ValueMatcher::static_string()))
        .collect();
    let constraints = CanonicalArgumentConstraints {
        groups: Box::new([ArgumentConstraintGroup {
            index: ArgumentIndex::new_unchecked(0),
            predicates: predicates.into_boxed_slice(),
        }]),
    };
    let plan = PhysicalPlan::new(
        Box::new([PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Global {
                name: "fetch".into(),
                strength: crate::api::compiler::rule::IdentityStrength::Strict,
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        }]),
        PlanRequirements::default(),
    );
    assert!(
        matches!(
            validate_physical_plan(&plan),
            Err(PhysicalPlanValidationError::ExcessivePredicateCount(_))
        ),
        "expected ExcessivePredicateCount error"
    );
}

#[test]
fn excessive_alternatives_are_rejected_at_construction() {
    let values: Vec<String> = (0..=limits::MAX_STATIC_ALTERNATIVES)
        .map(|i| format!("val{i}"))
        .collect();
    assert!(matches!(
        ValueMatcher::static_string().equals_any(values),
        Err(crate::api::rule::QueryBuildError::CollectionTooLarge(_, _))
    ));
}
