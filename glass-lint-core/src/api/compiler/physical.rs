use glass_lint_datastructures::SymbolPath;

#[cfg(test)]
use crate::api::rule::ArgumentConstraint;
use crate::api::{
    compiler::{
        error::PhysicalPlanValidationError,
        limits as compiler_limits,
        normalized::{CanonicalArgumentConstraints, ObjectSlot as NormalizedObjectSlot},
        object_flow::CompiledObjectFlow,
        requirements::PlanRequirements,
        rule::{EventSpec, EvidenceDescriptor, IdentityConstraint},
    },
    rule::query::limits,
};

mod planner;
mod validation;

#[cfg(test)]
pub(crate) use planner::plan_normalized;
pub(crate) use planner::plan_normalized_roots_into;
#[cfg(test)]
pub(crate) use validation::validate_physical_plan;
use validation::{requirements_for_roots, validate_canonical_constraints};

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
        event: EventSpec,
        evidence: EvidenceDescriptor,
    },
    ConstrainedScan {
        identity: IdentityConstraint,
        event: EventSpec,
        constraints: CanonicalArgumentConstraints,
        evidence: EvidenceDescriptor,
    },
    ReturnedSubject {
        producer: IdentityConstraint,
        object_slot: ObjectSlot,
        member: SymbolPath,
        event: EventSpec,
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
        event: EventSpec,
        evidence: EvidenceDescriptor,
    ) -> Self {
        Self::IndexedScan {
            identity,
            event,
            evidence,
        }
    }

    fn constrained_scan(
        identity: IdentityConstraint,
        event: EventSpec,
        constraints: CanonicalArgumentConstraints,
        evidence: EvidenceDescriptor,
    ) -> Self {
        Self::ConstrainedScan {
            identity,
            event,
            constraints,
            evidence,
        }
    }

    pub(crate) fn returned_subject(
        producer: IdentityConstraint,
        object_slot: NormalizedObjectSlot,
        member: SymbolPath,
        event: EventSpec,
        evidence: EvidenceDescriptor,
    ) -> Result<Self, PhysicalPlanValidationError> {
        Ok(Self::ReturnedSubject {
            producer,
            object_slot: ObjectSlot::new(object_slot.get())?,
            member,
            event,
            evidence,
        })
    }

    fn instance_subject(
        constructor: IdentityConstraint,
        object_slot: NormalizedObjectSlot,
        member: SymbolPath,
        evidence: EvidenceDescriptor,
    ) -> Result<Self, PhysicalPlanValidationError> {
        Ok(Self::InstanceSubject {
            constructor,
            object_slot: ObjectSlot::new(object_slot.get())?,
            member,
            evidence,
        })
    }

    /// Describe the preparation capabilities owned by this executable root.
    /// Keeping this mapping on the physical operator makes the executable
    /// plan the single source of truth for runtime requirements.
    fn merge_requirements_into(&self, requirements: &mut PlanRequirements) {
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
                if !matches!(event, EventSpec::Call | EventSpec::MemberCall { .. }) {
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
                if !matches!(event, EventSpec::MemberCall { member: event_member }
                    | EventSpec::MemberRead { member: event_member } if event_member == member)
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

#[derive(Debug, Default)]
pub(crate) struct RootBudget {
    used: usize,
}

impl RootBudget {
    pub(crate) const fn new() -> Self {
        Self { used: 0 }
    }

    pub(crate) fn reserve(&mut self) -> Result<(), PhysicalPlanValidationError> {
        if self.used >= compiler_limits::MAX_PHYSICAL_ROOTS_PER_RULE {
            return Err(PhysicalPlanValidationError::TooManyRoots(self.used + 1));
        }
        self.used += 1;
        Ok(())
    }
}

impl PhysicalPlan {
    /// Seal roots produced by the normalized-query planner.
    ///
    /// The production compiler validates the planned roots at this one
    /// sealing boundary. `from_roots` remains the independent validation
    /// boundary for callers that can supply physical roots directly.
    pub(crate) fn from_planned_roots(
        roots: Box<[PhysicalRoot]>,
    ) -> Result<Self, PhysicalPlanValidationError> {
        validate_root_set(&roots)?;
        Ok(Self::from_validated_roots(roots))
    }

    #[cfg(test)]
    pub(crate) fn from_roots(
        roots: Box<[PhysicalRoot]>,
    ) -> Result<Self, PhysicalPlanValidationError> {
        Self::from_planned_roots(roots)
    }

    fn from_validated_roots(roots: Box<[PhysicalRoot]>) -> Self {
        let requirements = requirements_for_roots(&roots);
        Self {
            roots,
            requirements,
        }
    }

    #[cfg(test)]
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
            "requirements value_resolution={:?} flow={{local={}, cross_call={}}} project={:?}",
            self.requirements.value_resolution(),
            self.requirements.flow().local(),
            self.requirements.flow().cross_call(),
            self.requirements.project_requirements(),
        ));
        lines.join("\n")
    }
}

fn validate_root_set(roots: &[PhysicalRoot]) -> Result<(), PhysicalPlanValidationError> {
    if roots.is_empty() {
        return Err(PhysicalPlanValidationError::EmptyRoots);
    }
    if roots.len() > compiler_limits::MAX_PHYSICAL_ROOTS_PER_RULE {
        return Err(PhysicalPlanValidationError::TooManyRoots(roots.len()));
    }
    for root in roots {
        root.validate()?;
    }
    Ok(())
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
