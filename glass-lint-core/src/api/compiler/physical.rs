//! Physical query plan types and the planner that produces them from
//! normalized logical queries.
//!
//! Physical operators correspond to executable execution paths through
//! the existing occurrence indexes, fact stream, and flow engine. The
//! planner selects the narrowest available index for each event/identity
//! pair and attaches same-event value predicates directly to the scan
//! or constrained projection.
//!
//! ## Layout
//!
//! The planner converts a normalized [`QueryDecl`] into
//! a [`PhysicalPlan`] containing zero or more [`PhysicalRoot`] values.
//! Each root is a self-contained executable operator.  Alternatives
//! (nested `Any`) are flattened into independent roots; same-variable
//! conjunctions (`All`) are merged into one root with combined
//! constraints.
//!
//! ## Physical operators
//!
//! | Operator | Purpose |
//! |---|---|
//! | `IndexedScan` | Unconstrained occurrence index lookup |
//! | `ConstrainedScan` | Call/member-call with argument constraints |
//! | `ReturnedSubject` | Member access on a returned object |
//! | `InstanceSubject` | Member call on a constructed instance |
//! | `Lifecycle` | Object flow lifecycle plan reference |

use glass_lint_datastructures::SymbolPath;

use crate::api::{
    classification::MatchKind,
    compiler::{
        normalize::PlanRequirements,
        object_flow::CompiledObjectFlow,
        rule::{
            EventPredicate, EvidenceDescriptor, IdentityConstraint, InvalidQueryClause,
            lower_event, lower_identity,
        },
    },
    rule::{
        ArgumentConstraint,
        query::{
            AllExpr, EventQuery, EventSpec, IdentitySpec, LifecycleQuery, QueryDecl, QueryExpr,
            QueryExprKind, QueryPredicate, SubjectSpec, VarId,
        },
    },
};

// ── Physical root types ─────────────────────────────────────────────────

/// A single executable physical operator root.
///
/// Each root can be executed independently by the appropriate analysis
/// subsystem.  The planner selects the narrowest possible operator for
/// each logical leaf.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PhysicalRoot {
    /// Indexed occurrence scan with no additional value constraints.
    ///
    /// Used for unconstrained calls, member calls, imports, strings,
    /// classes, and constructions — the fastest execution path.
    IndexedScan {
        identity: IdentityConstraint,
        event: EventPredicate,
        evidence: EvidenceDescriptor,
    },
    /// Constrained call projection with argument value constraints.
    ///
    /// Used for calls and member-calls with one or more value/argument
    /// predicates attached to the same event.
    ConstrainedScan {
        identity: IdentityConstraint,
        event: EventPredicate,
        constraints: Box<[ArgumentConstraint]>,
        evidence: EvidenceDescriptor,
    },
    /// Subject scan for member access on a returned object.
    ///
    /// The `identity` holds the rooted path of the producer (e.g.
    /// `document.createElement`); the `member` is the subsequent
    /// member access on the returned object.
    ReturnedSubject {
        identity: IdentityConstraint,
        member: SymbolPath,
        event: EventPredicate,
        evidence: EvidenceDescriptor,
    },
    /// Subject scan for a member call on a constructed instance.
    ///
    /// The `constructor` holds the module identity of the constructor
    /// (e.g. `ModuleExport("pkg", "Client")`); the `member` is the
    /// method called on the instance.
    InstanceSubject {
        constructor: IdentityConstraint,
        member: SymbolPath,
        evidence: EvidenceDescriptor,
    },
    /// Compiled lifecycle flow plan.
    Lifecycle { flow: CompiledObjectFlow },
}

// PhysicalRoot derives PartialOrd/Ord through its fields, which gives
// deterministic ordering by discriminant then by field values.

// ── PhysicalPlan ────────────────────────────────────────────────────────

/// Compiled physical plan containing executable roots and their
/// resource requirements.
///
/// A plan is produced by the planner from a normalized logical query.
/// It is stored in [`CompiledMatcherPlan`] and consumed by the analysis
/// layer during per-file matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhysicalPlan {
    /// Physical roots in deterministic execution order.
    roots: Box<[PhysicalRoot]>,
    /// Plan-wide resource requirements computed from the logical query.
    requirements: PlanRequirements,
}

impl PhysicalPlan {
    /// Create a new physical plan from roots and requirements.
    pub(crate) fn new(roots: Box<[PhysicalRoot]>, requirements: PlanRequirements) -> Self {
        Self {
            roots,
            requirements,
        }
    }

    /// Access physical roots in deterministic order.
    pub(crate) fn roots(&self) -> &[PhysicalRoot] {
        &self.roots
    }

    /// Access plan-wide resource requirements.
    pub(crate) fn requirements(&self) -> &PlanRequirements {
        &self.requirements
    }

    /// Return true when the plan has no executable roots.
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// A stable, deterministic plan summary string for tests and
    /// profiling.
    ///
    /// Format:
    /// ```text
    /// roots=N indexed_scans=N constrained_scans=N returned_subjects=N instance_subjects=N lifecycle_plans=N project_overlay=yes|no
    /// ```
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
                PhysicalRoot::Lifecycle { flow: _ } => lifecycle += 1,
            }
        }
        format!(
            "roots={} indexed_scans={} constrained_scans={} returned_subjects={} instance_subjects={} lifecycle_plans={} project_overlay={} cross_call_flow={}",
            self.roots.len(),
            indexed,
            constrained,
            returned,
            instance,
            lifecycle,
            if self.requirements.needs_project_overlay {
                "yes"
            } else {
                "no"
            },
            if self.requirements.needs_cross_call_flow {
                "yes"
            } else {
                "no"
            },
        )
    }
}

// ── Planner ─────────────────────────────────────────────────────────────

/// Plan a single event query into a physical root.
fn plan_event_query(eq: &EventQuery, kind: MatchKind, symbol: &str) -> Vec<PhysicalRoot> {
    let evidence = EvidenceDescriptor {
        kind,
        symbol: symbol.to_owned(),
    };

    if !eq.constraints.is_empty() {
        // Constrained call or member-call: compile to ConstrainedScan.
        return vec![PhysicalRoot::ConstrainedScan {
            identity: lower_identity(&eq.identity),
            event: lower_event(&eq.event),
            constraints: eq.constraints.iter().cloned().collect(),
            evidence,
        }];
    }

    match &eq.subject {
        SubjectSpec::Direct => {
            vec![PhysicalRoot::IndexedScan {
                identity: lower_identity(&eq.identity),
                event: lower_event(&eq.event),
                evidence,
            }]
        }
        SubjectSpec::ReturnedFrom => {
            let member = match &eq.event {
                EventSpec::MemberCall { member } | EventSpec::MemberRead { member } => {
                    member.clone()
                }
                _ => SymbolPath::default(),
            };
            vec![PhysicalRoot::ReturnedSubject {
                identity: lower_identity(&eq.identity),
                member,
                event: lower_event(&eq.event),
                evidence,
            }]
        }
        SubjectSpec::InstanceOf => {
            let member = match &eq.event {
                EventSpec::MemberCall { member } => member.clone(),
                _ => SymbolPath::default(),
            };
            vec![PhysicalRoot::InstanceSubject {
                constructor: lower_identity(&eq.identity),
                member,
                evidence,
            }]
        }
    }
}

/// Plan a normalized logical expression tree into a vector of physical
/// roots.
///
/// Nested `Any` branches are flattened into independent roots (the
/// planner receives already-flattened input from the normalizer).
/// Same-variable `All` branches are merged into a single root with
/// combined constraints.
fn plan_expression(expr: &QueryExpr, kind: MatchKind, symbol: &str) -> Vec<PhysicalRoot> {
    match &expr.kind {
        QueryExprKind::Event(eq) => plan_event_query(eq, kind, symbol),
        QueryExprKind::SelectEvent(_) | QueryExprKind::Require(_) => Vec::new(),
        QueryExprKind::Any(any) => {
            let mut roots = Vec::new();
            for branch in &any.branches {
                roots.extend(plan_expression(branch, kind, symbol));
            }
            roots
        }
        QueryExprKind::All(all) => plan_all_expression(all, kind, symbol),
        QueryExprKind::Lifecycle(lc) => {
            vec![plan_lifecycle(lc, kind, symbol)]
        }
    }
}

/// Plan a normalized `All` expression by merging same-variable branches
/// into one physical root.
///
/// After normalization, `All` only contains `Event` leaves (nested `All`
/// is flattened).  Branches sharing a variable represent predicates on
/// the same event that can be merged into a single scan with combined
/// constraints.
fn plan_all_expression(all: &AllExpr, kind: MatchKind, symbol: &str) -> Vec<PhysicalRoot> {
    // Try to extract an event from the All branches.
    // Handle both forms:
    // 1. Old form: branches contain Event(EventQuery) nodes
    // 2. New atomized form: branches contain SelectEvent + Require atoms
    let mut event_var: Option<VarId> = None;
    let mut event_spec: Option<EventSpec> = None;
    let mut identity_spec: Option<IdentitySpec> = None;
    let mut subject_spec: SubjectSpec = SubjectSpec::Direct;
    let mut constraints: Vec<ArgumentConstraint> = Vec::new();

    for branch in &all.branches {
        match &branch.kind {
            QueryExprKind::Event(eq) => {
                // Old form: carry the EventQuery fields.
                if event_var.is_none() {
                    event_var = Some(eq.var);
                    event_spec = Some(eq.event.clone());
                    identity_spec = Some(eq.identity.clone());
                    subject_spec = eq.subject;
                }
                constraints.extend(eq.constraints.iter().cloned());
            }
            QueryExprKind::SelectEvent(s) => {
                event_var = Some(s.bind);
            }
            QueryExprKind::Require(p) => match p {
                QueryPredicate::EventKind { expected, .. } => {
                    event_spec = Some(expected.clone());
                }
                QueryPredicate::EventIdentity { expected, .. } => {
                    identity_spec = Some(expected.clone());
                }
                QueryPredicate::Argument {
                    call: _,
                    index,
                    matcher,
                } => {
                    constraints.push(ArgumentConstraint::new(*index, matcher.clone()));
                }
                QueryPredicate::ReturnedObject { .. } => {
                    subject_spec = SubjectSpec::ReturnedFrom;
                }
                QueryPredicate::ConstructedObject { .. } => {
                    subject_spec = SubjectSpec::InstanceOf;
                }
                QueryPredicate::MemberSubject { .. } => {}
            },
            _ => {
                // Non-Event/non-atom branch (e.g. nested Any inside All):
                // plan independently and return.
                let mut roots = plan_atomized_event(
                    event_var,
                    event_spec,
                    identity_spec.as_ref(),
                    subject_spec,
                    &constraints,
                    kind,
                    symbol,
                );
                roots.extend(plan_expression(branch, kind, symbol));
                return roots;
            }
        }
    }

    plan_atomized_event(
        event_var,
        event_spec,
        identity_spec.as_ref(),
        subject_spec,
        &constraints,
        kind,
        symbol,
    )
}

/// Build a physical root from atomized event fields (extracted from
/// SelectEvent + Require atoms or from EventQuery).
fn plan_atomized_event(
    _event_var: Option<VarId>,
    event_spec: Option<EventSpec>,
    identity_spec: Option<&IdentitySpec>,
    _subject_spec: SubjectSpec,
    constraints: &[ArgumentConstraint],
    kind: MatchKind,
    symbol: &str,
) -> Vec<PhysicalRoot> {
    let Some(event) = event_spec else {
        return Vec::new();
    };
    let Some(identity) = identity_spec else {
        return Vec::new();
    };

    let evidence = EvidenceDescriptor {
        kind,
        symbol: symbol.to_owned(),
    };

    if constraints.is_empty() {
        vec![PhysicalRoot::IndexedScan {
            identity: lower_identity(identity),
            event: lower_event(&event),
            evidence,
        }]
    } else {
        vec![PhysicalRoot::ConstrainedScan {
            identity: lower_identity(identity),
            event: lower_event(&event),
            constraints: constraints.to_vec().into_boxed_slice(),
            evidence,
        }]
    }
}

/// Plan a normalized [`QueryDecl`] into a [`PhysicalPlan`].
///
/// The input should already be normalized (flattened, deduplicated,
/// sorted, with dense variable slots).  If the declaration contains
/// a lifecycle, the caller must assign flow indices via
/// [`patch_lifecycle_flow_indices`] before using the plan.
pub(crate) fn plan_normalized(decl: &QueryDecl, requirements: PlanRequirements) -> PhysicalPlan {
    let roots = plan_expression(&decl.expression, decl.emission.kind, &decl.emission.symbol);
    PhysicalPlan::new(roots.into_boxed_slice(), requirements)
}

/// Plan a lifecycle query into a [`PhysicalRoot::Lifecycle`] with an
/// embedded [`CompiledObjectFlow`].
fn plan_lifecycle(lc: &LifecycleQuery, _kind: MatchKind, symbol: &str) -> PhysicalRoot {
    PhysicalRoot::Lifecycle {
        flow: CompiledObjectFlow::from_lifecycle_query(lc, symbol),
    }
}

// ── Validation ──────────────────────────────────────────────────────────

/// Validate a [`PhysicalPlan`] for internal consistency.
///
/// Checks:
/// - No root has an empty identity.
/// - `ConstrainedScan` roots have a call-bearing event.
/// - `ReturnedSubject` roots have a member-call or member-read event.
/// - `InstanceSubject` roots have a member-call event.
/// - Lifecycle has non-empty sources.
pub(crate) fn validate_physical_plan(
    plan: &PhysicalPlan,
    _flow_count: usize,
) -> Result<(), InvalidQueryClause> {
    for root in plan.roots() {
        match root {
            PhysicalRoot::IndexedScan { identity, .. } => {
                if identity.is_empty() {
                    return Err(InvalidQueryClause::ImpossibleDimensions);
                }
            }
            PhysicalRoot::ConstrainedScan {
                identity, event, ..
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
            }
            PhysicalRoot::ReturnedSubject {
                identity, event, ..
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
            }
            PhysicalRoot::InstanceSubject { constructor, .. } => {
                if constructor.is_empty() {
                    return Err(InvalidQueryClause::ImpossibleDimensions);
                }
            }
            PhysicalRoot::Lifecycle { flow } => {
                if flow.sources.is_empty() {
                    return Err(InvalidQueryClause::ImpossibleDimensions);
                }
            }
        }
    }
    Ok(())
}

// ── Backward-compat conversion: physical roots → clauses ─────────────
// (removed in Phase 7 — analysis layer uses physical roots directly)

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        classification::MatchKind,
        compiler::{normalize::normalize_query_decl, rule::IdentityStrength},
        rule::{
            QueryDecl, ValueMatcher,
            query::{EmissionDecl, IdentitySpec, QueryBuildError, VarId},
        },
    };

    // ── Helpers ────────────────────────────────────────────────────

    fn physical_summary(decl: &QueryDecl) -> String {
        let (normalized, req) = normalize_query_decl(decl);
        let plan = plan_normalized(&normalized, req);
        plan.summary()
    }

    fn physical_roots(decl: &QueryDecl) -> Vec<PhysicalRoot> {
        let (normalized, req) = normalize_query_decl(decl);
        let plan = plan_normalized(&normalized, req);
        plan.roots().to_vec()
    }

    // ── Planner selects the expected physical access path ──────────

    fn decl(decl: Result<QueryDecl, QueryBuildError>) -> QueryDecl {
        decl.unwrap()
    }

    #[test]
    fn global_call_produces_indexed_scan() {
        let roots = physical_roots(&decl(QueryDecl::call_global("fetch")));
        assert_eq!(roots.len(), 1);
        assert!(
            matches!(&roots[0], PhysicalRoot::IndexedScan { .. }),
            "expected IndexedScan, got {roots:?}"
        );
    }

    #[test]
    fn heuristic_call_produces_indexed_scan() {
        let roots = physical_roots(&decl(QueryDecl::call_heuristic("fetch")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn constrained_call_produces_constrained_scan() {
        let decl = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string())
            .unwrap()
            .into_query();
        let roots = physical_roots(&decl);
        assert_eq!(roots.len(), 1);
        assert!(
            matches!(&roots[0], PhysicalRoot::ConstrainedScan { .. }),
            "expected ConstrainedScan, got {roots:?}"
        );
    }

    #[test]
    fn rooted_member_call_produces_indexed_scan() {
        let roots = physical_roots(&decl(QueryDecl::member_call_rooted(
            "document.createElement",
        )));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn returned_subject_produces_returned_scan() {
        let roots = physical_roots(&decl(QueryDecl::member_call_returned("create", "send")));
        assert_eq!(roots.len(), 1);
        assert!(
            matches!(&roots[0], PhysicalRoot::ReturnedSubject { .. }),
            "expected ReturnedSubject, got {roots:?}"
        );
    }

    #[test]
    fn instance_subject_produces_instance_scan() {
        let roots = physical_roots(&decl(QueryDecl::member_call_instance(
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
        let roots = physical_roots(&decl(QueryDecl::import_exact("node:fs")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn string_contains_produces_indexed_scan() {
        let roots = physical_roots(&decl(QueryDecl::string_contains("https://")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn class_reference_produces_indexed_scan() {
        let roots = physical_roots(&decl(QueryDecl::class_heuristic("Worker")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn constructor_global_produces_indexed_scan() {
        let roots = physical_roots(&decl(QueryDecl::constructor_global("URL")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn module_call_produces_indexed_scan() {
        let roots = physical_roots(&decl(QueryDecl::call_module("fs", "readFile")));
        assert_eq!(roots.len(), 1);
        assert!(matches!(&roots[0], PhysicalRoot::IndexedScan { .. }));
    }

    #[test]
    fn member_read_returned_produces_returned_scan() {
        let roots = physical_roots(&decl(QueryDecl::member_read_returned("create", "token")));
        assert_eq!(roots.len(), 1);
        assert!(
            matches!(&roots[0], PhysicalRoot::ReturnedSubject { .. }),
            "expected ReturnedSubject, got {roots:?}"
        );
    }

    // ── Same-event filters fuse into one constrained operator ─────

    #[test]
    fn multiple_constraints_on_same_call_fuse_into_one_constrained_scan() {
        let decl = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string())
            .unwrap()
            .with_arg(1, ValueMatcher::static_string().equals("/api"))
            .unwrap()
            .into_query();
        let roots = physical_roots(&decl);
        assert_eq!(roots.len(), 1);
        match &roots[0] {
            PhysicalRoot::ConstrainedScan { constraints, .. } => {
                assert_eq!(constraints.len(), 2);
            }
            other => panic!("expected ConstrainedScan, got {other:?}"),
        }
    }

    // ── Alternatives retain deterministic order ───────────────────

    #[test]
    fn alternatives_from_any_produce_multiple_roots() {
        use crate::api::rule::query::{AnyExpr, EventQuery, EventSpec, QueryExpr};
        let branches = vec![
            QueryExpr::event(EventQuery {
                var: VarId::new(0),
                event: EventSpec::Call,
                identity: IdentitySpec::Global {
                    name: "fetch".into(),
                },
                subject: SubjectSpec::Direct,
                constraints: vec![],
            }),
            QueryExpr::event(EventQuery {
                var: VarId::new(1),
                event: EventSpec::Call,
                identity: IdentitySpec::Global {
                    name: "navigate".into(),
                },
                subject: SubjectSpec::Direct,
                constraints: vec![],
            }),
        ];
        let query = QueryDecl {
            expression: QueryExpr::any(AnyExpr::new(branches).unwrap()),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "request".into(),
            },
        };
        let (normalized, req) = normalize_query_decl(&query);
        let plan = plan_normalized(&normalized, req);
        // Two alternatives → two physical roots
        assert_eq!(plan.roots().len(), 2);
        for root in plan.roots() {
            assert!(
                matches!(root, PhysicalRoot::IndexedScan { .. }),
                "expected IndexedScan for each alternative"
            );
        }
    }

    // ── Plan summary tests ────────────────────────────────────────

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
        let decl = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string())
            .unwrap()
            .into_query();
        let summary = physical_summary(&decl);
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

    // ── Validation tests ──────────────────────────────────────────

    #[test]
    fn empty_identity_fails_validation() {
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
            validate_physical_plan(&plan, 0),
            Err(InvalidQueryClause::ImpossibleDimensions)
        );
    }

    #[test]
    fn valid_roots_pass_validation() {
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
        assert!(validate_physical_plan(&plan, 0).is_ok());
    }

    // ── Roots-to-clauses conversion ───────────────────────────────
    // (Removed in Phase 7 — analysis layer uses physical roots directly)

    // ── Planner equivalence tests ─────────────────────────────────

    #[test]
    fn equivalent_declarations_produce_identical_plans() {
        let roots1 = physical_roots(&decl(QueryDecl::call_global("fetch")));
        let roots2 = physical_roots(&decl(QueryDecl::call_global("fetch")));
        assert_eq!(roots1, roots2);
    }

    #[test]
    fn different_declarations_produce_different_plans() {
        let roots1 = physical_roots(&decl(QueryDecl::call_global("fetch")));
        let roots2 = physical_roots(&decl(QueryDecl::call_global("navigate")));
        assert_ne!(roots1, roots2);
    }

    #[test]
    fn plan_summary_is_stable_across_equal_queries() {
        let s1 = physical_summary(&decl(QueryDecl::call_global("fetch")));
        let s2 = physical_summary(&decl(QueryDecl::call_global("fetch")));
        assert_eq!(s1, s2);
    }
}
