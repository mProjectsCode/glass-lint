use glass_lint_datastructures::SymbolPath;

#[cfg(test)]
use crate::api::rule::ArgumentConstraint;
use crate::api::{
    classification::MatchKind,
    compiler::{
        error::PhysicalPlanValidationError,
        normalized::{
            CanonicalArgumentConstraints, NormalizedEvent, NormalizedLifecycle, NormalizedQuery,
            NormalizedRoot,
        },
        object_flow::CompiledObjectFlow,
        requirements::PlanRequirements,
        rule::{
            EventPredicate, EvidenceDescriptor, IdentityConstraint, lower_event, lower_identity,
        },
        validate::{SubjectRelation, classify_subject_relation},
    },
    rule::{
        ArgumentIndex, ArgumentMatcher, ArgumentMatcherKind, StaticStringPredicateKind,
        ValueMatcherKind, query::limits,
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
        object_slot: ObjectSlot,
        member: SymbolPath,
        event: EventPredicate,
        evidence: EvidenceDescriptor,
    },
    InstanceSubject {
        constructor: IdentityConstraint,
        object_slot: ObjectSlot,
        member: SymbolPath,
        evidence: EvidenceDescriptor,
    },
    Lifecycle {
        flow: CompiledObjectFlow,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ObjectSlot(u32);

impl ObjectSlot {
    fn new(slot: u32) -> Result<Self, PhysicalPlanValidationError> {
        (slot != u32::MAX)
            .then_some(Self(slot))
            .ok_or(PhysicalPlanValidationError::ImpossibleDimensions)
    }
}

impl std::fmt::Display for ObjectSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl PhysicalRoot {
    fn indexed_scan(
        identity: IdentityConstraint,
        event: EventPredicate,
        evidence: EvidenceDescriptor,
    ) -> Result<Self, PhysicalPlanValidationError> {
        Self::validated(Self::IndexedScan {
            identity,
            event,
            evidence,
        })
    }

    fn constrained_scan(
        identity: IdentityConstraint,
        event: EventPredicate,
        constraints: CanonicalArgumentConstraints,
        evidence: EvidenceDescriptor,
    ) -> Result<Self, PhysicalPlanValidationError> {
        Self::validated(Self::ConstrainedScan {
            identity,
            event,
            constraints,
            evidence,
        })
    }

    pub(crate) fn returned_subject(
        producer: IdentityConstraint,
        object_slot: u32,
        member: SymbolPath,
        event: EventPredicate,
        evidence: EvidenceDescriptor,
    ) -> Result<Self, PhysicalPlanValidationError> {
        Self::validated(Self::ReturnedSubject {
            producer,
            object_slot: ObjectSlot::new(object_slot)?,
            member,
            event,
            evidence,
        })
    }

    fn instance_subject(
        constructor: IdentityConstraint,
        object_slot: u32,
        member: SymbolPath,
        evidence: EvidenceDescriptor,
    ) -> Result<Self, PhysicalPlanValidationError> {
        Self::validated(Self::InstanceSubject {
            constructor,
            object_slot: ObjectSlot::new(object_slot)?,
            member,
            evidence,
        })
    }

    fn validated(root: Self) -> Result<Self, PhysicalPlanValidationError> {
        root.validate()?;
        Ok(root)
    }

    /// Describe the preparation capabilities owned by this executable root.
    /// Keeping this mapping on the physical operator makes the executable
    /// plan the single source of truth for runtime requirements.
    fn requirements(&self) -> PlanRequirements {
        let mut requirements = PlanRequirements::default();
        match self {
            Self::IndexedScan { identity, .. } => requirements.require_identity(identity),
            Self::ConstrainedScan { identity, .. } => {
                requirements.require_local_static_values();
                requirements.require_identity(identity);
            }
            Self::ReturnedSubject { .. } => {}
            Self::InstanceSubject { constructor, .. } => {
                requirements.require_identity(constructor);
            }
            Self::Lifecycle { .. } => {
                requirements.require_local_flow();
                requirements.require_cross_call_flow();
            }
        }
        requirements
    }

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
                object_slot: _,
                member,
                event,
                evidence,
                ..
            } => {
                if producer.is_empty()
                    || !matches!(producer, IdentityConstraint::Rooted { .. })
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
                object_slot: _,
                member,
                evidence,
                ..
            } => {
                if constructor.is_empty()
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
    fn from_roots(roots: Box<[PhysicalRoot]>) -> Result<Self, PhysicalPlanValidationError> {
        let requirements = requirements_for_roots(&roots);
        let plan = Self {
            roots,
            requirements,
        };
        validate_physical_plan(&plan)?;
        Ok(plan)
    }

    pub(crate) fn try_new(
        roots: Box<[PhysicalRoot]>,
        requirements: &PlanRequirements,
    ) -> Result<Self, PhysicalPlanValidationError> {
        let plan = Self::from_roots(roots)?;
        if plan.requirements != *requirements {
            return Err(PhysicalPlanValidationError::RequirementsMismatch);
        }
        Ok(plan)
    }

    #[cfg(test)]
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
pub(crate) fn plan_normalized(
    nq: &NormalizedQuery,
) -> Result<PhysicalPlan, PhysicalPlanValidationError> {
    let emission = nq.emission();
    let kind = emission.kind();
    let symbol = emission.symbol();
    let roots = plan_root(nq.root(), kind, symbol)?;
    PhysicalPlan::from_roots(roots.into_boxed_slice())
}

fn plan_root(
    root: &NormalizedRoot,
    kind: MatchKind,
    symbol: &str,
) -> Result<Vec<PhysicalRoot>, PhysicalPlanValidationError> {
    match root {
        NormalizedRoot::Event(ev) => plan_event(ev, kind, symbol),
        NormalizedRoot::Any(branches) => {
            let mut roots = Vec::new();
            for b in branches {
                roots.extend(plan_root(b, kind, symbol)?);
            }
            Ok(roots)
        }
        NormalizedRoot::Lifecycle(lc) => Ok(vec![plan_lifecycle(lc, symbol)?]),
    }
}

fn plan_event(
    ev: &NormalizedEvent,
    kind: MatchKind,
    symbol: &str,
) -> Result<Vec<PhysicalRoot>, PhysicalPlanValidationError> {
    let relation = classify_subject_relation(ev.event(), ev.subject())
        .map_err(|_| PhysicalPlanValidationError::ImpossibleDimensions)?;
    let evidence = EvidenceDescriptor {
        kind,
        symbol: symbol.to_owned(),
    };

    match relation {
        SubjectRelation::Direct { identity } => {
            if ev.arguments().is_empty() {
                Ok(vec![PhysicalRoot::indexed_scan(
                    lower_identity(identity),
                    lower_event(ev.event()),
                    evidence,
                )?])
            } else {
                Ok(vec![PhysicalRoot::constrained_scan(
                    lower_identity(identity),
                    lower_event(ev.event()),
                    ev.arguments().clone(),
                    evidence,
                )?])
            }
        }
        SubjectRelation::Returned {
            producer,
            object_slot,
            member,
            event,
        } => Ok(vec![PhysicalRoot::returned_subject(
            lower_identity(producer),
            object_slot,
            member.clone(),
            lower_event(event),
            evidence,
        )?]),
        SubjectRelation::Instance {
            constructor,
            object_slot,
            member,
        } => Ok(vec![PhysicalRoot::instance_subject(
            lower_identity(constructor),
            object_slot,
            member.clone(),
            evidence,
        )?]),
    }
}

fn plan_lifecycle(
    lc: &NormalizedLifecycle,
    symbol: &str,
) -> Result<PhysicalRoot, PhysicalPlanValidationError> {
    CompiledObjectFlow::from_normalized_lifecycle(lc, symbol)
        .map(|flow| PhysicalRoot::Lifecycle { flow })
        .map_err(
            |error| PhysicalPlanValidationError::InvalidLifecycleSource {
                detail: error.detail(),
            },
        )
}

// ── Validation ──────────────────────────────────────────────────────────

pub(crate) fn validate_physical_plan(
    plan: &PhysicalPlan,
) -> Result<(), PhysicalPlanValidationError> {
    for root in plan.roots() {
        root.validate()?;
    }
    if requirements_for_roots(plan.roots()) != *plan.requirements() {
        return Err(PhysicalPlanValidationError::RequirementsMismatch);
    }
    Ok(())
}

fn requirements_for_roots(roots: &[PhysicalRoot]) -> PlanRequirements {
    let mut requirements = PlanRequirements::default();
    for root in roots {
        requirements.merge_from(&root.requirements());
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
