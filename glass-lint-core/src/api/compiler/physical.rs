use glass_lint_datastructures::SymbolPath;

#[cfg(test)]
use crate::api::rule::ArgumentConstraint;
use crate::api::{
    classification::MatchKind,
    compiler::{
        error::PhysicalPlanValidationError,
        normalized::{
            CanonicalArgumentConstraints, NormalizedEvent, NormalizedLifecycle, NormalizedQuery,
            NormalizedRoot, NormalizedSubject,
        },
        object_flow::CompiledObjectFlow,
        requirements::PlanRequirements,
        rule::{
            EventPredicate, EvidenceDescriptor, IdentityConstraint, lower_event, lower_identity,
        },
    },
    rule::{
        ArgumentIndex, ArgumentMatcher, ArgumentMatcherKind, StaticStringPredicateKind,
        ValueMatcherKind,
        query::{EventSpec, limits},
    },
};

/// Compile raw argument constraints into canonical grouped form.
///
/// The input slice does not need to be pre-canonicalized (sorted,
/// deduplicated). The canonical constraint owner handles both operations.
#[cfg(test)]
pub(crate) fn compile_argument_constraints(
    raw: &[ArgumentConstraint],
) -> CanonicalArgumentConstraints {
    CanonicalArgumentConstraints::from_constraints(raw)
}

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
        constraints: CanonicalArgumentConstraints,
        evidence: EvidenceDescriptor,
    },
    ReturnedSubject {
        producer: IdentityConstraint,
        object_slot: u32,
        member: SymbolPath,
        event: EventPredicate,
        evidence: EvidenceDescriptor,
    },
    InstanceSubject {
        constructor: IdentityConstraint,
        object_slot: u32,
        member: SymbolPath,
        evidence: EvidenceDescriptor,
    },
    Lifecycle {
        flow: CompiledObjectFlow,
    },
}

impl PhysicalRoot {
    fn validate(&self) -> Result<(), PhysicalPlanValidationError> {
        match self {
            Self::IndexedScan {
                identity, evidence, ..
            } => {
                if identity.is_empty() {
                    return Err(PhysicalPlanValidationError::ImpossibleDimensions);
                }
                if evidence.symbol.is_empty() {
                    return Err(PhysicalPlanValidationError::UnavailablePrimaryEvidence);
                }
            }
            Self::ConstrainedScan {
                identity,
                event,
                constraints,
                evidence,
            } => {
                if identity.is_empty() {
                    return Err(PhysicalPlanValidationError::ImpossibleDimensions);
                }
                if !matches!(
                    event,
                    EventPredicate::Call | EventPredicate::MemberCall { .. }
                ) {
                    return Err(PhysicalPlanValidationError::ConstraintsRequireCallEvent);
                }
                if constraints.is_empty() {
                    return Err(PhysicalPlanValidationError::NonCanonicalConstraints);
                }
                validate_canonical_constraints(constraints)?;
                if evidence.symbol.is_empty() {
                    return Err(PhysicalPlanValidationError::UnavailablePrimaryEvidence);
                }
            }
            Self::ReturnedSubject {
                producer,
                object_slot,
                member,
                event,
                evidence,
                ..
            } => {
                if producer.is_empty()
                    || !matches!(producer, IdentityConstraint::Rooted { .. })
                    || *object_slot == u32::MAX
                    || member.is_empty()
                {
                    return Err(PhysicalPlanValidationError::ImpossibleDimensions);
                }
                if !matches!(event, EventPredicate::MemberCall { member: event_member }
                    | EventPredicate::MemberRead { member: event_member } if event_member == member)
                {
                    return Err(PhysicalPlanValidationError::ImpossibleDimensions);
                }
                if evidence.symbol.is_empty() {
                    return Err(PhysicalPlanValidationError::UnavailablePrimaryEvidence);
                }
            }
            Self::InstanceSubject {
                constructor,
                object_slot,
                member,
                evidence,
                ..
            } => {
                if constructor.is_empty()
                    || *object_slot == u32::MAX
                    || !matches!(
                        constructor,
                        IdentityConstraint::ModuleExport { .. }
                            | IdentityConstraint::PackageModuleExport { .. }
                    )
                    || member.is_empty()
                {
                    return Err(PhysicalPlanValidationError::ImpossibleDimensions);
                }
                if evidence.symbol.is_empty() {
                    return Err(PhysicalPlanValidationError::UnavailablePrimaryEvidence);
                }
            }
            Self::Lifecycle { flow } => {
                if !flow.has_sources() {
                    return Err(PhysicalPlanValidationError::InvalidLifecycleRoot);
                }
                if flow.requirement_count() > limits::MAX_LIFECYCLE_EVENTS
                    || flow.sink_count() > limits::MAX_LIFECYCLE_SINKS
                {
                    return Err(PhysicalPlanValidationError::ExcessiveLifecycleEvidence {
                        requirements: flow.requirement_count(),
                        sinks: flow.sink_count(),
                    });
                }
            }
        }
        Ok(())
    }
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

    #[cfg(test)]
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
            "roots={} indexed_scans={} constrained_scans={} returned_subjects={} instance_subjects={} lifecycle_plans={} local_flow={} cross_call_flow={} project_overlay={} value_resolution={:?} project_requirements={:?}",
            self.roots.len(),
            indexed,
            constrained,
            returned,
            instance,
            lifecycle,
            if self.requirements.flow().local() {
                "yes"
            } else {
                "no"
            },
            if self.requirements.flow().cross_call() {
                "yes"
            } else {
                "no"
            },
            if self.requirements.needs_project_overlay() {
                "yes"
            } else {
                "no"
            },
            self.requirements.value_resolution(),
            self.requirements.project_requirements(),
        )
    }

    /// Return a deterministic, human-readable explanation of the executable
    /// plan. This intentionally describes semantic operators and requirements,
    /// rather than exposing compiler slots or index implementation details.
    #[cfg(test)]
    pub(crate) fn explain(&self) -> String {
        let mut lines = vec![format!("plan {}", self.summary())];
        lines.push("optimization canonical-root-order,deduplicate-identical-roots".into());
        for (index, root) in self.roots.iter().enumerate() {
            lines.push(format!("root[{index}] {}", explain_root(root)));
        }
        lines.push(format!(
            "requirements value_resolution={:?} flow={{local={}, cross_call={}, cross_file={}}} project={:?}",
            self.requirements.value_resolution(),
            self.requirements.flow().local(),
            self.requirements.flow().cross_call(),
            self.requirements.flow().cross_file(),
            self.requirements.project_requirements(),
        ));
        lines.join("\n")
    }
}

/// Apply deterministic, semantics-preserving physical plan optimizations.
///
/// Roots are order-independent alternatives, so canonical sorting and exact
/// deduplication are safe. Evidence-bearing roots are only deduplicated when
/// their complete descriptors are equal; roots with different evidence remain
/// separate. This deliberately keeps the unoptimized semantic representation
/// available to tests while making the production choice explicit.
pub(crate) fn optimize_roots(mut roots: Vec<PhysicalRoot>) -> Box<[PhysicalRoot]> {
    roots.sort();
    roots.dedup();
    roots.into_boxed_slice()
}

#[cfg(test)]
fn explain_root(root: &PhysicalRoot) -> String {
    match root {
        PhysicalRoot::IndexedScan {
            identity,
            event,
            evidence,
        } => format!(
            "indexed event={event:?} identity={identity:?} evidence={:?}:{}",
            evidence.kind, evidence.symbol
        ),
        PhysicalRoot::ConstrainedScan {
            identity,
            event,
            constraints,
            evidence,
        } => format!(
            "constrained event={event:?} identity={identity:?} groups={} evidence={:?}:{}",
            constraints.groups().len(),
            evidence.kind,
            evidence.symbol
        ),
        PhysicalRoot::ReturnedSubject {
            producer,
            member,
            event,
            object_slot,
            evidence,
        } => format!(
            "returned_subject producer={producer:?} member={member} event={event:?} slot={object_slot} evidence={:?}:{}",
            evidence.kind, evidence.symbol
        ),
        PhysicalRoot::InstanceSubject {
            constructor,
            member,
            object_slot,
            evidence,
        } => format!(
            "instance_subject constructor={constructor:?} member={member} slot={object_slot} evidence={:?}:{}",
            evidence.kind, evidence.symbol
        ),
        PhysicalRoot::Lifecycle { flow } => format!("lifecycle {flow:?}"),
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
        NormalizedRoot::Lifecycle(lc) => plan_lifecycle(lc, symbol).into_iter().collect(),
    }
}

fn plan_event(ev: &NormalizedEvent, kind: MatchKind, symbol: &str) -> Vec<PhysicalRoot> {
    let evidence = EvidenceDescriptor {
        kind,
        symbol: symbol.to_owned(),
    };

    match ev.subject() {
        NormalizedSubject::Direct { identity } => {
            if ev.arguments().is_empty() {
                vec![PhysicalRoot::IndexedScan {
                    identity: lower_identity(identity),
                    event: lower_event(ev.event()),
                    evidence,
                }]
            } else {
                vec![PhysicalRoot::ConstrainedScan {
                    identity: lower_identity(identity),
                    event: lower_event(ev.event()),
                    constraints: ev.arguments().clone(),
                    evidence,
                }]
            }
        }
        NormalizedSubject::Returned {
            producer,
            object_slot,
        } => {
            let member = match ev.event() {
                EventSpec::MemberCall { member } | EventSpec::MemberRead { member } => {
                    member.clone()
                }
                _ => SymbolPath::default(),
            };
            vec![PhysicalRoot::ReturnedSubject {
                producer: lower_identity(producer),
                object_slot: *object_slot,
                member,
                event: lower_event(ev.event()),
                evidence,
            }]
        }
        NormalizedSubject::Instance {
            constructor,
            object_slot,
        } => {
            let member = match ev.event() {
                EventSpec::MemberCall { member } => member.clone(),
                _ => SymbolPath::default(),
            };
            vec![PhysicalRoot::InstanceSubject {
                constructor: lower_identity(constructor),
                object_slot: *object_slot,
                member,
                evidence,
            }]
        }
    }
}

fn plan_lifecycle(lc: &NormalizedLifecycle, symbol: &str) -> Option<PhysicalRoot> {
    CompiledObjectFlow::from_normalized_lifecycle(lc, symbol)
        .map(|flow| PhysicalRoot::Lifecycle { flow })
}

// ── Validation ──────────────────────────────────────────────────────────

pub(crate) fn validate_physical_plan(
    plan: &PhysicalPlan,
) -> Result<(), PhysicalPlanValidationError> {
    for root in plan.roots() {
        root.validate()?;
    }
    if executable_requirements(plan.roots()) != *plan.requirements() {
        return Err(PhysicalPlanValidationError::RequirementsMismatch);
    }
    Ok(())
}

fn executable_requirements(roots: &[PhysicalRoot]) -> PlanRequirements {
    let mut requirements = PlanRequirements::default();
    for root in roots {
        match root {
            PhysicalRoot::ConstrainedScan { identity, .. }
            | PhysicalRoot::IndexedScan { identity, .. } => {
                if matches!(root, PhysicalRoot::ConstrainedScan { .. }) {
                    requirements.require_local_static_values();
                }
                requirements.require_identity(identity);
            }
            PhysicalRoot::ReturnedSubject { .. } => {}
            PhysicalRoot::InstanceSubject { constructor, .. } => {
                requirements.require_identity(constructor);
            }
            PhysicalRoot::Lifecycle { .. } => {
                requirements.require_local_flow();
                requirements.require_cross_call_flow();
            }
        }
    }
    requirements
}

/// Validate that compiled constraints are well-formed.
///
/// Groups must be non-empty, in ascending index order, with at least one
/// predicate per group and no empty predicates.  Group and predicate counts
/// must be within declared limits.
fn validate_canonical_constraints(
    constraints: &CanonicalArgumentConstraints,
) -> Result<(), PhysicalPlanValidationError> {
    let groups = constraints.groups();
    if groups.is_empty() {
        return Err(PhysicalPlanValidationError::NonCanonicalConstraints);
    }

    if groups.len() > limits::MAX_ARGUMENT_GROUPS {
        return Err(PhysicalPlanValidationError::ExcessiveArgumentGroups(
            groups.len(),
        ));
    }

    let mut prev_index: Option<ArgumentIndex> = None;
    for group in groups {
        if group.predicates().is_empty() {
            return Err(PhysicalPlanValidationError::NonCanonicalConstraints);
        }
        if group.predicates().len() > limits::MAX_PREDICATES_PER_ARGUMENT {
            return Err(PhysicalPlanValidationError::ExcessivePredicateCount(
                group.predicates().len(),
            ));
        }
        if let Some(prev) = prev_index
            && prev >= group.index()
        {
            return Err(PhysicalPlanValidationError::NonCanonicalConstraints);
        }
        prev_index = Some(group.index());

        // Check static-string alternative limits per predicate
        for matcher in group.predicates() {
            if let Some(count) = count_matcher_alternatives(matcher)
                && count > limits::MAX_STATIC_ALTERNATIVES
            {
                return Err(PhysicalPlanValidationError::ExcessiveAlternatives(count));
            }
        }
    }
    Ok(())
}

/// Count the number of static-string alternatives in an argument matcher, if
/// applicable.
fn count_matcher_alternatives(matcher: &ArgumentMatcher) -> Option<usize> {
    match matcher.kind() {
        ArgumentMatcherKind::Value(vm) => match vm.kind() {
            ValueMatcherKind::StaticString(sp) => match sp.kind() {
                StaticStringPredicateKind::Exact(v)
                | StaticStringPredicateKind::Prefix(v)
                | StaticStringPredicateKind::ContainsAny(v)
                | StaticStringPredicateKind::ContainsAll(v) => Some(v.len()),
                StaticStringPredicateKind::Any => None,
            },
            ValueMatcherKind::Any => None,
        },
        _ => None,
    }
}
