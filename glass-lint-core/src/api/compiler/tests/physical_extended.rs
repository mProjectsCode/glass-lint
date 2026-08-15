use super::*;

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
fn malformed_lifecycle_source_is_reported_instead_of_dropped() {
    let query = crate::api::compiler::normalized::NormalizedQuery::new(
        NormalizedRoot::Lifecycle(NormalizedLifecycle::new(
            vec![NormalizedEvent::new(
                EventSlot::from_raw(0),
                crate::api::rule::query::EventSpec::PropertyWrite {
                    property: SymbolPath::from("config.mode"),
                },
                crate::api::compiler::normalized::NormalizedSubject::Direct {
                    identity: crate::api::rule::query::IdentitySpec::Global {
                        name: "config".into(),
                    },
                },
                CanonicalArgumentConstraints::default(),
            )],
            None,
            Some(crate::api::compiler::normalized::NormalizedLifecycleCompletion::Configuration),
        )),
        NormalizedEmission::new(MatchKind::Call, "lifecycle".into()),
    );

    assert_eq!(
        plan_normalized(&query),
        Err(PhysicalPlanValidationError::InvalidLifecycleSource {
            detail: "lifecycle source must be a global call or rooted member call",
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
        .map(|i| {
            ArgumentConstraintGroup::new_for_test(
                ArgumentIndex::new_unchecked(i as u8),
                Box::new([ArgumentMatcher::from(ValueMatcher::static_string())]),
            )
        })
        .collect();
    let constraints = CanonicalArgumentConstraints::from_groups_for_test(groups.into_boxed_slice());
    let plan = PhysicalPlan::new(
        Box::new([PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Global {
                name: "fetch".into(),
            },
            event: EventSpec::Call,
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
    let constraints = CanonicalArgumentConstraints::from_groups_for_test(Box::new([
        ArgumentConstraintGroup::new_for_test(
            ArgumentIndex::new_unchecked(0),
            predicates.into_boxed_slice(),
        ),
    ]));
    let plan = PhysicalPlan::new(
        Box::new([PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Global {
                name: "fetch".into(),
            },
            event: EventSpec::Call,
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
