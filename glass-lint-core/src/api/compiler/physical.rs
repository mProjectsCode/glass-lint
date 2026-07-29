use glass_lint_datastructures::SymbolPath;

use crate::api::{
    classification::MatchKind,
    compiler::{
        error::PhysicalPlanValidationError,
        normalized::{
            NormalizedEvent, NormalizedLifecycle, NormalizedQuery, NormalizedRoot,
            NormalizedSubject,
        },
        object_flow::CompiledObjectFlow,
        requirements::{PlanRequirements, ProjectRequirement, ValueResolutionRequirement},
        rule::{
            EventPredicate, EvidenceDescriptor, IdentityConstraint, lower_event, lower_identity,
        },
    },
    rule::{
        ArgumentConstraint, ArgumentIndex, ArgumentMatcher, ArgumentMatcherKind,
        StaticStringPredicateKind, ValueMatcherKind,
        query::{EventSpec, limits},
    },
};

// ── Compiled argument constraints ───────────────────────────────────────

/// Compiled argument constraints, grouped and deduplicated by argument index.
///
/// Groups are stored in deterministic index order. Each argument is prepared
/// at most once during evaluation — all predicates in a group are applied to
/// one prepared `ArgumentView`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CompiledArgumentConstraints {
    pub(crate) groups: Box<[ArgumentConstraintGroup]>,
}

/// A group of predicates all applying to the same argument index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArgumentConstraintGroup {
    pub(crate) index: ArgumentIndex,
    pub(crate) predicates: Box<[ArgumentMatcher]>,
}

impl CompiledArgumentConstraints {
    pub(crate) fn groups(&self) -> &[ArgumentConstraintGroup] {
        &self.groups
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

impl ArgumentConstraintGroup {
    pub(crate) fn index(&self) -> ArgumentIndex {
        self.index
    }

    pub(crate) fn predicates(&self) -> &[ArgumentMatcher] {
        &self.predicates
    }
}

/// Compile raw argument constraints into grouped, deduplicated form.
///
/// Normalization already sorts, deduplicates, and validates constraints.
/// This function groups them by index, moves predicates into one
/// group per unique index (preserving normalized order), validates
/// bounds, and detects remaining contradictions.
pub(crate) fn compile_argument_constraints(
    raw: &[ArgumentConstraint],
) -> CompiledArgumentConstraints {
    let mut groups: Vec<ArgumentConstraintGroup> = Vec::new();
    for constraint in raw {
        let idx = constraint.arg_index();
        let matcher = constraint.predicate().clone();
        if let Some(last) = groups.last_mut()
            && last.index() == idx
        {
            // Same index — deduplicate against last predicate
            if !last.predicates.contains(&matcher) {
                let mut predicates = last.predicates.to_vec();
                predicates.push(matcher);
                last.predicates = predicates.into_boxed_slice();
            }
        } else {
            groups.push(ArgumentConstraintGroup {
                index: idx,
                predicates: Box::new([matcher]),
            });
        }
    }

    CompiledArgumentConstraints {
        groups: groups.into_boxed_slice(),
    }
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
        constraints: CompiledArgumentConstraints,
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
            "roots={} indexed_scans={} constrained_scans={} returned_subjects={} instance_subjects={} lifecycle_plans={} local_flow={} cross_call_flow={} project_overlay={} value_resolution={:?} project_requirements={:?}",
            self.roots.len(),
            indexed,
            constrained,
            returned,
            instance,
            lifecycle,
            if self.requirements.flow().local {
                "yes"
            } else {
                "no"
            },
            if self.requirements.flow().cross_call {
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
    #[allow(dead_code)]
    pub(crate) fn explain(&self) -> String {
        let mut lines = vec![format!("plan {}", self.summary())];
        lines.push("optimization canonical-root-order,deduplicate-identical-roots".into());
        for (index, root) in self.roots.iter().enumerate() {
            lines.push(format!("root[{index}] {}", explain_root(root)));
        }
        lines.push(format!(
            "requirements value_resolution={:?} flow={{local={}, cross_call={}, cross_file={}}} project={:?}",
            self.requirements.value_resolution(),
            self.requirements.flow().local,
            self.requirements.flow().cross_call,
            self.requirements.flow().cross_file,
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

#[allow(dead_code)]
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
                    identity: lower_identity(
                        ev.identity()
                            .expect("direct normalized events retain an identity"),
                    ),
                    event: lower_event(ev.event()),
                    evidence,
                }]
            } else {
                vec![PhysicalRoot::ConstrainedScan {
                    identity: lower_identity(
                        ev.identity()
                            .expect("direct normalized events retain an identity"),
                    ),
                    event: lower_event(ev.event()),
                    constraints: compile_argument_constraints(ev.arguments()),
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

fn plan_lifecycle(lc: &NormalizedLifecycle, symbol: &str) -> PhysicalRoot {
    // Convert NormalizedLifecycle back to a LifecycleQuery for compilation.
    let sources: Vec<crate::api::rule::query::EventQuery> = lc
        .sources()
        .iter()
        .map(|sev| crate::api::rule::query::EventQuery {
            var: crate::api::rule::query::VarId::new(sev.slot()),
            event: sev.event().clone(),
            identity: sev
                .identity()
                .expect("lifecycle sources retain an identity")
                .clone(),
            constraints: sev.arguments().to_vec(),
        })
        .collect();

    let lc_query = crate::api::rule::query::LifecycleQuery::new(
        "lifecycle",
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

pub(crate) fn validate_physical_plan(
    plan: &PhysicalPlan,
) -> Result<(), PhysicalPlanValidationError> {
    for root in plan.roots() {
        match root {
            PhysicalRoot::IndexedScan {
                identity, evidence, ..
            } => {
                if identity.is_empty() {
                    return Err(PhysicalPlanValidationError::ImpossibleDimensions);
                }
                if evidence.symbol.is_empty() {
                    return Err(PhysicalPlanValidationError::UnavailablePrimaryEvidence);
                }
            }
            PhysicalRoot::ConstrainedScan {
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
            PhysicalRoot::ReturnedSubject {
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
            PhysicalRoot::InstanceSubject {
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
            PhysicalRoot::Lifecycle { flow } => {
                if flow.sources.is_empty() {
                    return Err(PhysicalPlanValidationError::InvalidLifecycleRoot);
                }
            }
        }
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
                    requirements
                        .value_resolution
                        .insert(ValueResolutionRequirement::LocalStaticValues);
                }
                add_identity_requirements(&mut requirements, identity);
            }
            PhysicalRoot::ReturnedSubject { .. } => {}
            PhysicalRoot::InstanceSubject { constructor, .. } => {
                add_identity_requirements(&mut requirements, constructor);
            }
            PhysicalRoot::Lifecycle { .. } => {
                requirements.flow.local = true;
                requirements.flow.cross_call = true;
            }
        }
    }
    requirements
}

fn add_identity_requirements(requirements: &mut PlanRequirements, identity: &IdentityConstraint) {
    match identity {
        IdentityConstraint::ModuleExport { .. } => {
            requirements.value_resolution.extend([
                ValueResolutionRequirement::ModuleIdentityValues,
                ValueResolutionRequirement::CallResultIdentities,
            ]);
            requirements.project.extend([
                ProjectRequirement::ExactModuleExports,
                ProjectRequirement::CallResultIdentities,
            ]);
        }
        IdentityConstraint::PackageModuleExport { .. } => {
            requirements.value_resolution.extend([
                ValueResolutionRequirement::ModuleIdentityValues,
                ValueResolutionRequirement::CallResultIdentities,
            ]);
            requirements.project.extend([
                ProjectRequirement::PackageModuleExports,
                ProjectRequirement::CallResultIdentities,
            ]);
        }
        IdentityConstraint::ModuleNamespace { .. } => {
            requirements
                .value_resolution
                .insert(ValueResolutionRequirement::ModuleIdentityValues);
            requirements
                .project
                .insert(ProjectRequirement::ExactModuleNamespaces);
        }
        IdentityConstraint::PackageModuleNamespace { .. } => {
            requirements
                .value_resolution
                .insert(ValueResolutionRequirement::ModuleIdentityValues);
            requirements
                .project
                .insert(ProjectRequirement::PackageModuleNamespaces);
        }
        _ => {}
    }
}

/// Validate that compiled constraints are well-formed.
///
/// Groups must be non-empty, in ascending index order, with at least one
/// predicate per group and no empty predicates.  Group and predicate counts
/// must be within declared limits.
fn validate_canonical_constraints(
    constraints: &CompiledArgumentConstraints,
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
        ArgumentMatcherKind::Value(vm) => match &vm.kind {
            ValueMatcherKind::StaticString(sp) => match &sp.kind {
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
