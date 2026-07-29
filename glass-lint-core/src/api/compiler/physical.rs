use glass_lint_datastructures::SymbolPath;

use crate::api::{
    classification::MatchKind,
    compiler::{
        normalize::{
            NormalizedEvent, NormalizedLifecycle, NormalizedQuery, NormalizedRoot,
            NormalizedSubject, PlanRequirements,
        },
        object_flow::CompiledObjectFlow,
        rule::{
            EventPredicate, EvidenceDescriptor, IdentityConstraint, InvalidQueryClause,
            lower_event, lower_identity,
        },
    },
    rule::query::EventSpec,
};

// ── Physical root types ─────────────────────────────────────────────────

/// A single executable physical operator root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PhysicalRoot {
    IndexedScan {
        identity: IdentityConstraint,
        event: EventPredicate,
        evidence: EvidenceDescriptor,
    },
    ConstrainedScan {
        identity: IdentityConstraint,
        event: EventPredicate,
        constraints: Box<[crate::api::rule::ArgumentConstraint]>,
        evidence: EvidenceDescriptor,
    },
    ReturnedSubject {
        identity: IdentityConstraint,
        member: SymbolPath,
        event: EventPredicate,
        evidence: EvidenceDescriptor,
    },
    InstanceSubject {
        constructor: IdentityConstraint,
        member: SymbolPath,
        evidence: EvidenceDescriptor,
    },
    Lifecycle {
        flow: CompiledObjectFlow,
    },
}

// ── PhysicalPlan ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhysicalPlan {
    roots: Box<[PhysicalRoot]>,
    requirements: PlanRequirements,
}

impl PhysicalPlan {
    pub(crate) fn new(roots: Box<[PhysicalRoot]>, requirements: PlanRequirements) -> Self {
        Self {
            roots,
            requirements,
        }
    }

    pub(crate) fn roots(&self) -> &[PhysicalRoot] {
        &self.roots
    }

    pub(crate) fn requirements(&self) -> &PlanRequirements {
        &self.requirements
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    #[allow(dead_code)]
    pub(crate) fn summary(&self) -> String {
        let mut indexed = 0usize;
        let mut constrained = 0usize;
        let mut returned = 0usize;
        let mut instance = 0usize;
        let mut lifecycle = 0usize;
        for root in &self.roots {
            match root {
                PhysicalRoot::IndexedScan { .. } => indexed += 1,
                PhysicalRoot::ConstrainedScan { .. } => constrained += 1,
                PhysicalRoot::ReturnedSubject { .. } => returned += 1,
                PhysicalRoot::InstanceSubject { .. } => instance += 1,
                PhysicalRoot::Lifecycle { .. } => lifecycle += 1,
            }
        }
        format!(
            "roots={} indexed_scans={} constrained_scans={} returned_subjects={} instance_subjects={} lifecycle_plans={} local_flow={} cross_call_flow={} project_overlay={}",
            self.roots.len(),
            indexed,
            constrained,
            returned,
            instance,
            lifecycle,
            if self.requirements.needs_local_flow {
                "yes"
            } else {
                "no"
            },
            if self.requirements.needs_cross_call_flow {
                "yes"
            } else {
                "no"
            },
            if self.requirements.needs_project_overlay {
                "yes"
            } else {
                "no"
            },
        )
    }
}

// ── Planner ─────────────────────────────────────────────────────────────

/// Plan a normalized query into a [`PhysicalPlan`].
pub(crate) fn plan_normalized(nq: &NormalizedQuery) -> PhysicalPlan {
    let emission = nq.emission();
    let kind = emission.kind();
    let symbol = emission.symbol();
    let roots = plan_root(nq.root(), kind, symbol);
    PhysicalPlan::new(roots.into_boxed_slice(), nq.requirements().clone())
}

fn plan_root(root: &NormalizedRoot, kind: MatchKind, symbol: &str) -> Vec<PhysicalRoot> {
    match root {
        NormalizedRoot::Event(ev) => plan_event(ev, kind, symbol),
        NormalizedRoot::Any(branches) => {
            let mut roots = Vec::new();
            for b in branches {
                roots.extend(plan_root(b, kind, symbol));
            }
            roots
        }
        NormalizedRoot::Lifecycle(lc) => {
            vec![plan_lifecycle(lc, symbol)]
        }
    }
}

fn plan_event(ev: &NormalizedEvent, kind: MatchKind, symbol: &str) -> Vec<PhysicalRoot> {
    let evidence = EvidenceDescriptor {
        kind,
        symbol: symbol.to_owned(),
    };

    match ev.subject() {
        NormalizedSubject::Direct => {
            if ev.arguments().is_empty() {
                vec![PhysicalRoot::IndexedScan {
                    identity: lower_identity(ev.identity()),
                    event: lower_event(ev.event()),
                    evidence,
                }]
            } else {
                vec![PhysicalRoot::ConstrainedScan {
                    identity: lower_identity(ev.identity()),
                    event: lower_event(ev.event()),
                    constraints: ev.arguments().to_vec().into_boxed_slice(),
                    evidence,
                }]
            }
        }
        NormalizedSubject::Returned => {
            let member = match ev.event() {
                EventSpec::MemberCall { member } | EventSpec::MemberRead { member } => {
                    member.clone()
                }
                _ => SymbolPath::default(),
            };
            vec![PhysicalRoot::ReturnedSubject {
                identity: lower_identity(ev.identity()),
                member,
                event: lower_event(ev.event()),
                evidence,
            }]
        }
        NormalizedSubject::Instance => {
            let member = match ev.event() {
                EventSpec::MemberCall { member } => member.clone(),
                _ => SymbolPath::default(),
            };
            vec![PhysicalRoot::InstanceSubject {
                constructor: lower_identity(ev.identity()),
                member,
                evidence,
            }]
        }
    }
}

fn plan_lifecycle(lc: &NormalizedLifecycle, symbol: &str) -> PhysicalRoot {
    // Convert NormalizedLifecycle back to a LifecycleQuery for compilation.
    let sources: Vec<crate::api::rule::query::EventQuery> = lc
        .sources()
        .iter()
        .map(|sev| crate::api::rule::query::EventQuery {
            var: crate::api::rule::query::VarId::new(sev.slot()),
            event: sev.event().clone(),
            identity: sev.identity().clone(),
            subject: match sev.subject() {
                NormalizedSubject::Direct => crate::api::rule::query::SubjectSpec::Direct,
                NormalizedSubject::Returned => crate::api::rule::query::SubjectSpec::ReturnedFrom,
                NormalizedSubject::Instance => crate::api::rule::query::SubjectSpec::InstanceOf,
            },
            constraints: sev.arguments().to_vec(),
        })
        .collect();

    let lc_query = crate::api::rule::query::LifecycleQuery::new(
        sources,
        lc.condition().cloned(),
        lc.completion().cloned(),
    )
    .expect("normalized lifecycle must be valid");
    PhysicalRoot::Lifecycle {
        flow: CompiledObjectFlow::from_lifecycle_query(&lc_query, symbol),
    }
}

// ── Validation ──────────────────────────────────────────────────────────

pub(crate) fn validate_physical_plan(plan: &PhysicalPlan) -> Result<(), InvalidQueryClause> {
    for root in plan.roots() {
        match root {
            PhysicalRoot::IndexedScan {
                identity, evidence, ..
            } => {
                if identity.is_empty() {
                    return Err(InvalidQueryClause::ImpossibleDimensions);
                }
                if evidence.symbol.is_empty() {
                    return Err(InvalidQueryClause::UnavailablePrimaryEvidence);
                }
            }
            PhysicalRoot::ConstrainedScan {
                identity,
                event,
                constraints,
                evidence,
            } => {
                if identity.is_empty() {
                    return Err(InvalidQueryClause::ImpossibleDimensions);
                }
                if !matches!(
                    event,
                    EventPredicate::Call | EventPredicate::MemberCall { .. }
                ) {
                    return Err(InvalidQueryClause::ConstraintsRequireCallEvent);
                }
                if constraints.is_empty() {
                    return Err(InvalidQueryClause::NonCanonicalConstraints);
                }
                validate_canonical_constraints(constraints)?;
                if evidence.symbol.is_empty() {
                    return Err(InvalidQueryClause::UnavailablePrimaryEvidence);
                }
            }
            PhysicalRoot::ReturnedSubject {
                identity,
                event,
                evidence,
                ..
            } => {
                if identity.is_empty() {
                    return Err(InvalidQueryClause::ImpossibleDimensions);
                }
                if !matches!(
                    event,
                    EventPredicate::MemberCall { .. } | EventPredicate::MemberRead { .. }
                ) {
                    return Err(InvalidQueryClause::ImpossibleDimensions);
                }
                if evidence.symbol.is_empty() {
                    return Err(InvalidQueryClause::UnavailablePrimaryEvidence);
                }
            }
            PhysicalRoot::InstanceSubject {
                constructor,
                evidence,
                ..
            } => {
                if constructor.is_empty() {
                    return Err(InvalidQueryClause::ImpossibleDimensions);
                }
                if evidence.symbol.is_empty() {
                    return Err(InvalidQueryClause::UnavailablePrimaryEvidence);
                }
            }
            PhysicalRoot::Lifecycle { flow } => {
                if flow.sources.is_empty() {
                    return Err(InvalidQueryClause::InvalidLifecycleRoot);
                }
            }
        }
    }
    Ok(())
}

/// Validate that constraints are in canonical order (sorted by index, then
/// matcher, with no duplicates).
fn validate_canonical_constraints(
    constraints: &[crate::api::rule::ArgumentConstraint],
) -> Result<(), InvalidQueryClause> {
    for pair in constraints.windows(2) {
        let ordering = pair[0]
            .index()
            .cmp(&pair[1].index())
            .then_with(|| pair[0].matcher().cmp(pair[1].matcher()));
        if ordering != std::cmp::Ordering::Less {
            return Err(InvalidQueryClause::NonCanonicalConstraints);
        }
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        classification::MatchKind,
        compiler::normalize::normalize_query_decl,
        rule::{
            QueryDecl, ValueMatcher,
            query::{QueryBuildError, VarId},
        },
    };

    fn physical_summary(decl: &QueryDecl) -> String {
        let nq = normalize_query_decl(decl).unwrap();
        let plan = plan_normalized(&nq);
        plan.summary()
    }

    fn physical_roots_from_decl(decl: &QueryDecl) -> Vec<PhysicalRoot> {
        let nq = normalize_query_decl(decl).unwrap();
        let plan = plan_normalized(&nq);
        plan.roots().to_vec()
    }

    fn decl(decl: Result<QueryDecl, QueryBuildError>) -> QueryDecl {
        decl.unwrap()
    }

    #[test]
    fn global_call_produces_indexed_scan() {
        let roots = physical_roots_from_decl(&decl(QueryDecl::call_global("fetch")));
        assert_eq!(roots.len(), 1);
        assert!(
            matches!(&roots[0], PhysicalRoot::IndexedScan { .. }),
            "expected IndexedScan, got {roots:?}"
        );
    }

    #[test]
    fn heuristic_call_produces_indexed_scan() {
        let roots = physical_roots_from_decl(&decl(QueryDecl::call_heuristic("fetch")));
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
        let roots = physical_roots_from_decl(&decl(QueryDecl::member_call_rooted(
            "document.createElement",
        )));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn returned_subject_produces_returned_scan() {
        let roots =
            physical_roots_from_decl(&decl(QueryDecl::member_call_returned("create", "send")));
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
        let roots = physical_roots_from_decl(&decl(QueryDecl::import_exact("node:fs")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn string_contains_produces_indexed_scan() {
        let roots = physical_roots_from_decl(&decl(QueryDecl::string_contains("https://")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn class_reference_produces_indexed_scan() {
        let roots = physical_roots_from_decl(&decl(QueryDecl::class_heuristic("Worker")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn constructor_global_produces_indexed_scan() {
        let roots = physical_roots_from_decl(&decl(QueryDecl::constructor_global("URL")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn module_call_produces_indexed_scan() {
        let roots = physical_roots_from_decl(&decl(QueryDecl::call_module("fs", "readFile")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn member_read_returned_produces_returned_scan() {
        let roots =
            physical_roots_from_decl(&decl(QueryDecl::member_read_returned("create", "token")));
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
            .with_arg(1, ValueMatcher::static_string().equals("/api"))
            .unwrap()
            .into_query();
        let roots = physical_roots_from_decl(&d);
        assert_eq!(roots.len(), 1);
        match &roots[0] {
            PhysicalRoot::ConstrainedScan { constraints, .. } => {
                assert_eq!(constraints.len(), 2);
            }
            other => panic!("expected ConstrainedScan, got {other:?}"),
        }
    }

    #[test]
    fn alternatives_from_any_produce_multiple_roots() {
        use crate::api::rule::query::{AnyExpr, EventQuery, EventSpec, QueryExpr};
        let branches = vec![
            QueryExpr::event(EventQuery {
                var: VarId::new(0),
                event: EventSpec::Call,
                identity: crate::api::rule::query::IdentitySpec::Global {
                    name: "fetch".into(),
                },
                subject: crate::api::rule::query::SubjectSpec::Direct,
                constraints: vec![],
            }),
            QueryExpr::event(EventQuery {
                var: VarId::new(1),
                event: EventSpec::Call,
                identity: crate::api::rule::query::IdentitySpec::Global {
                    name: "navigate".into(),
                },
                subject: crate::api::rule::query::SubjectSpec::Direct,
                constraints: vec![],
            }),
        ];
        let query = QueryDecl {
            expression: QueryExpr::any(AnyExpr::new(branches).unwrap()),
            emission: crate::api::rule::query::EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "request".into(),
            },
        };
        let nq = normalize_query_decl(&query).unwrap();
        let plan = plan_normalized(&nq);
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
        let summary = physical_summary(&decl(QueryDecl::call_global("fetch")));
        assert!(summary.contains("roots=1"), "summary: {summary}");
        assert!(summary.contains("indexed_scans=1"), "summary: {summary}");
        assert!(
            summary.contains("constrained_scans=0"),
            "summary: {summary}"
        );
        assert!(
            summary.contains("returned_subjects=0"),
            "summary: {summary}"
        );
        assert!(
            summary.contains("instance_subjects=0"),
            "summary: {summary}"
        );
        assert!(summary.contains("project_overlay=no"), "summary: {summary}");
    }

    #[test]
    fn plan_summary_shows_constrained_scan() {
        let d = crate::api::rule::EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string())
            .unwrap()
            .into_query();
        let summary = physical_summary(&d);
        assert!(summary.contains("roots=1"), "summary: {summary}");
        assert!(
            summary.contains("constrained_scans=1"),
            "summary: {summary}"
        );
        assert!(summary.contains("indexed_scans=0"), "summary: {summary}");
    }

    #[test]
    fn plan_summary_shows_project_overlay_for_module_queries() {
        let summary = physical_summary(&decl(QueryDecl::call_module("fs", "readFile")));
        assert!(
            summary.contains("project_overlay=yes"),
            "summary: {summary}"
        );
    }

    #[test]
    fn plan_summary_shows_no_project_overlay_for_global_queries() {
        let summary = physical_summary(&decl(QueryDecl::call_global("fetch")));
        assert!(summary.contains("project_overlay=no"), "summary: {summary}");
    }

    #[test]
    fn empty_identity_fails_validation() {
        use crate::api::compiler::rule::IdentityStrength;
        let roots = Box::new([PhysicalRoot::IndexedScan {
            identity: IdentityConstraint::Global {
                name: "".into(),
                strength: IdentityStrength::Strict,
            },
            event: EventPredicate::Call,
            evidence: EvidenceDescriptor {
                kind: MatchKind::Call,
                symbol: "test".into(),
            },
        }]);
        let plan = PhysicalPlan::new(roots, PlanRequirements::default());
        assert_eq!(
            validate_physical_plan(&plan),
            Err(InvalidQueryClause::ImpossibleDimensions)
        );
    }

    #[test]
    fn valid_roots_pass_validation() {
        use crate::api::compiler::rule::IdentityStrength;
        let roots = Box::new([PhysicalRoot::IndexedScan {
            identity: IdentityConstraint::Global {
                name: "fetch".into(),
                strength: IdentityStrength::Strict,
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
    fn equivalent_declarations_produce_identical_plans() {
        let roots1 = physical_roots_from_decl(&decl(QueryDecl::call_global("fetch")));
        let roots2 = physical_roots_from_decl(&decl(QueryDecl::call_global("fetch")));
        assert_eq!(roots1, roots2);
    }

    #[test]
    fn different_declarations_produce_different_plans() {
        let roots1 = physical_roots_from_decl(&decl(QueryDecl::call_global("fetch")));
        let roots2 = physical_roots_from_decl(&decl(QueryDecl::call_global("navigate")));
        assert_ne!(roots1, roots2);
    }

    #[test]
    fn plan_summary_is_stable_across_equal_queries() {
        let s1 = physical_summary(&decl(QueryDecl::call_global("fetch")));
        let s2 = physical_summary(&decl(QueryDecl::call_global("fetch")));
        assert_eq!(s1, s2);
    }
}
