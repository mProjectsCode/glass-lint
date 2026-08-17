//! Compiled matcher overlays and project-level matcher evidence.
//!
//! Projection is deliberately after local fact construction and project
//! linking. It applies qualified identities once, composes bounded flow, and
//! leaves rule selection to the compiled matcher catalog.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

mod outcome;
pub use outcome::ProjectionOutcome;

use crate::{
    analysis::{
        ModuleId, ProjectModule, ProjectSemanticModel,
        facts::SemanticFacts,
        flow::{
            self,
            planning::BoundLifecycleRoot,
            projector::{self as object_flow, LocalFlowProjectionOutcome},
        },
        matching::{
            ConstrainedRootInput, MatcherOverlayPolicy, MatcherProjectContext,
            MatcherProjectOverlay,
        },
        model::flow::FlowLimits,
        project::{model::MAX_EXPORT_LOOKUP_ENTRIES, state::ExportLookupCache},
        trace::TraceArena,
    },
    api::{
        classification::{
            ClassificationEvidence, ClassificationResult, MatchedCapability, RuleEvidenceCapacity,
            RuleEvidenceError, RuleEvidenceTable, RuleIndex,
        },
        compiler::{
            CompiledMatcherPlan, CompiledRuleSelection, physical::PhysicalRoot,
            requirements::PlanRequirements,
        },
    },
};

pub(in crate::analysis) fn project_for_classification<'project, 'matchers>(
    project: &'project ProjectSemanticModel,
    matchers: CompiledRuleSelection<'matchers>,
) -> (
    ProjectMatcherModel<'project, 'matchers>,
    ProjectionOutcome,
    TraceArena,
) {
    let trace_limit = project.trace_limit();
    let mut arena = TraceArena::new(trace_limit);
    let (catalog, outcome) = project.project_with_arena(matchers, &mut arena);
    (catalog, outcome, arena)
}

pub(in crate::analysis) fn assemble_classification_results(
    matcher_catalog: &ProjectMatcherModel<'_, '_>,
    evidence_limit: usize,
) -> BTreeMap<ModuleId, ClassificationResult> {
    matcher_catalog
        .modules()
        .map(|module| {
            let mut result = ClassificationResult::default();
            for (rule_index, record) in matcher_catalog.selection().selected_records() {
                let evidence =
                    matcher_catalog.evidence_for_lossy(module, rule_index, evidence_limit);
                if evidence.is_empty() {
                    continue;
                }

                result.push_capability(MatchedCapability::new(
                    rule_index,
                    record.description.clone(),
                    record.severity,
                    evidence,
                ));
            }
            (module.id(), result)
        })
        .collect()
}

/// Flattened matcher requirements shared by all module projections in one
/// linked project. The plan belongs to projection orchestration rather than
/// to the immutable facts artifact.
pub(in crate::analysis) struct ProjectionPlan<'a> {
    constrained_roots: Vec<ConstrainedRootInput<'a>>,
    flow_matchers: Vec<BoundLifecycleRoot<'a>>,
    rule_capacity: RuleEvidenceCapacity,
    requirements: PlanRequirements,
}

struct ProjectionSession<'project, 'plan, 'roots, 'arena> {
    project: &'project ProjectSemanticModel,
    plan: &'plan ProjectionPlan<'roots>,
    flow_limits: FlowLimits,
    arena: &'arena mut TraceArena,
    linking: ExportLookupCache,
}

impl<'project, 'plan, 'roots, 'arena> ProjectionSession<'project, 'plan, 'roots, 'arena> {
    fn new(
        project: &'project ProjectSemanticModel,
        plan: &'plan ProjectionPlan<'roots>,
        flow_limits: FlowLimits,
        arena: &'arena mut TraceArena,
    ) -> Self {
        Self {
            project,
            plan,
            flow_limits,
            arena,
            linking: ExportLookupCache::new(MAX_EXPORT_LOOKUP_ENTRIES),
        }
    }

    fn collect_cross(
        &mut self,
        roots: &[BoundLifecycleRoot<'roots>],
        capacity: RuleEvidenceCapacity,
    ) -> (
        BTreeMap<ModuleId, RuleEvidenceTable>,
        flow::cross::CrossProjectionOutcome,
    ) {
        flow::cross::collect(self.project, roots, capacity, &mut self.linking, self.arena)
    }

    fn project_modules<'module>(
        &'module mut self,
    ) -> Result<
        (
            BTreeMap<ModuleId, ProjectModuleProjection<'project>>,
            ProjectionOutcome,
        ),
        RuleEvidenceError,
    > {
        let requirements = &self.plan.requirements;
        let need_module_ids =
            requirements.needs_module_identities() || requirements.needs_project_overlay();
        let need_result_ids = requirements.needs_call_result_identities();
        let mut outcome = ProjectionOutcome::default();
        let projections = self
            .project
            .modules()
            .map(|module| -> Result<_, RuleEvidenceError> {
                let facts = module.local().facts();
                let projectable = facts.is_projectable();
                let identities = (projectable && need_module_ids).then(|| {
                    self.project
                        .module_identities(module.id(), &mut self.linking)
                });
                let result_identities = (projectable && need_result_ids).then(|| {
                    self.project
                        .call_result_identities(module.id(), &mut self.linking)
                });
                let overlay_policy = if requirements.needs_project_overlay() {
                    MatcherOverlayPolicy::Enabled
                } else {
                    MatcherOverlayPolicy::Disabled
                };
                let project_overlay =
                    MatcherProjectOverlay::new(identities.as_ref(), result_identities.as_ref());
                let matcher_context =
                    MatcherProjectContext::from_facts(facts, project_overlay, overlay_policy);
                let effects = self.plan.needs_flow().then(|| module.local().effects());
                if let Some(effects) = effects
                    && effects.is_available()
                    && effects.completion().is_incomplete()
                {
                    outcome.record_effects(module.id(), effects);
                }
                let (projected, local) = project_facts(
                    facts,
                    effects,
                    self.plan,
                    self.flow_limits,
                    module.id(),
                    self.arena,
                    &matcher_context,
                )?;
                outcome.record_local(&local);
                let matcher_artifact = matcher_context.into_artifact();
                Ok((
                    module.id(),
                    ProjectModuleProjection {
                        module,
                        matcher_artifact,
                        projected,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok((projections, outcome))
    }
}

impl<'a> ProjectionPlan<'a> {
    pub(in crate::analysis) fn needs_flow(&self) -> bool {
        !self.flow_matchers.is_empty()
    }

    pub(in crate::analysis) fn from_selection(selection: &'a CompiledRuleSelection<'a>) -> Self {
        let mut constrained_roots = Vec::new();
        let mut flow_matchers = Vec::new();
        let mut requirements = PlanRequirements::default();
        for (rule_index, matcher) in selection.selected_matchers() {
            for (root_index, root) in matcher.physical_roots().iter().enumerate() {
                match root {
                    PhysicalRoot::ConstrainedScan { constraints, .. }
                        if !constraints.groups().is_empty() =>
                    {
                        constrained_roots.push(ConstrainedRootInput::new(rule_index, root));
                    }
                    PhysicalRoot::Lifecycle { flow } => {
                        flow_matchers.push(BoundLifecycleRoot::new(rule_index, root_index, flow));
                    }
                    _ => {}
                }
            }
            requirements.merge_from(matcher.requirements());
        }
        Self {
            constrained_roots,
            flow_matchers,
            rule_capacity: selection.evidence_capacity(),
            requirements,
        }
    }
}

/// Execute the query-selected local matching and flow projection for one
/// immutable facts artifact.
fn project_facts(
    facts: &SemanticFacts,
    effects: Option<&crate::analysis::flow::effect::FunctionEffects>,
    plan: &ProjectionPlan<'_>,
    flow_limits: FlowLimits,
    module_id: ModuleId,
    trace_arena: &mut TraceArena,
    matcher_context: &MatcherProjectContext<'_, '_>,
) -> Result<(RuleEvidenceTable, LocalFlowProjectionOutcome), RuleEvidenceError> {
    let mut projected_evidence = RuleEvidenceTable::new(plan.rule_capacity);
    if !facts.is_projectable() {
        return Ok((projected_evidence, LocalFlowProjectionOutcome::default()));
    }
    crate::analysis::matching::try_compute_constrained_evidence(
        matcher_context.artifact(),
        &plan.constrained_roots,
        &mut projected_evidence,
        matcher_context.project(),
    )?;
    let Some(effects) = effects.filter(|effects| effects.is_available()) else {
        return Ok((projected_evidence, LocalFlowProjectionOutcome::default()));
    };
    let outcome = object_flow::collect_into(
        facts.stream(),
        effects,
        &plan.flow_matchers,
        &mut projected_evidence,
        flow_limits,
        module_id,
        trace_arena,
    );
    Ok((projected_evidence, outcome))
}

#[derive(Debug)]
/// Matcher-independent facts and cross-file evidence for one linked project.
pub(in crate::analysis) struct ProjectMatcherModel<'project, 'matchers> {
    identity: ProjectMatcherIdentity,
    matchers: CompiledRuleSelection<'matchers>,
    projections: BTreeMap<ModuleId, ProjectModuleProjection<'project>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProjectMatcherIdentity(u64);

static NEXT_PROJECT_MATCHER_ID: AtomicU64 = AtomicU64::new(1);

impl ProjectMatcherIdentity {
    fn next() -> Self {
        Self(NEXT_PROJECT_MATCHER_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis) enum EvidenceQueryError {
    ForeignModel,
    UnselectedRule,
    UnknownRule,
    UnknownModule,
}

/// An opaque reference to a module retained by one projection model.
#[derive(Clone, Copy, Debug)]
pub(in crate::analysis) struct ProjectModuleHandle<'project> {
    module: &'project ProjectModule,
    owner: ProjectMatcherIdentity,
}

impl ProjectModuleHandle<'_> {
    pub(in crate::analysis) fn id(self) -> ModuleId {
        self.module.id()
    }
}

#[derive(Debug)]
struct ProjectModuleProjection<'project> {
    module: &'project ProjectModule,
    matcher_artifact: crate::analysis::matching::MatcherArtifact<'project>,
    projected: RuleEvidenceTable,
}

impl ProjectModuleProjection<'_> {
    fn evidence_for(
        &self,
        matcher: &CompiledMatcherPlan,
        rule_index: RuleIndex,
    ) -> Vec<ClassificationEvidence> {
        let mut evidence = self
            .matcher_artifact
            .indexes()
            .evidence_for_indexed_with_overlay(
                crate::analysis::matching::IndexedRootIter::from_plan(matcher),
                self.matcher_artifact.overlay(),
                self.module.local().facts().names(),
            );

        if let Some(projected) = self.projected.for_rule(rule_index) {
            evidence.extend_from_slice(projected);
        }

        evidence
    }
}

/// Side effects produced by a projection that were previously written back
/// into the project model through hidden interior mutability.  The caller
/// decides how to merge or report these instead of the project mutating
/// itself through a shared reference.
impl ProjectSemanticModel {
    /// Project a linked semantic model into matcher queries without rewalking
    /// any source AST.  Side effects such as budget exhaustion and projection
    /// counts are returned in a `ProjectionOutcome` instead of being written
    /// back into `self`.
    #[cfg(test)]
    pub(in crate::analysis) fn project<'project, 'matchers>(
        &'project self,
        matchers: CompiledRuleSelection<'matchers>,
    ) -> (ProjectMatcherModel<'project, 'matchers>, ProjectionOutcome) {
        let mut arena = TraceArena::new(self.trace_limit());
        self.project_with_arena(matchers, &mut arena)
    }

    pub(in crate::analysis) fn project_with_arena<'project, 'matchers>(
        &'project self,
        matchers: CompiledRuleSelection<'matchers>,
        arena: &mut TraceArena,
    ) -> (ProjectMatcherModel<'project, 'matchers>, ProjectionOutcome) {
        let plan = ProjectionPlan::from_selection(&matchers);
        let flow_limits = FlowLimits::from_flow_operations(self.flow_limit());
        let has_flow = plan.needs_flow();
        let mut session = ProjectionSession::new(self, &plan, flow_limits, arena);
        let (projections, mut outcome) = match session.project_modules() {
            Ok(result) => result,
            Err(error) => {
                let mut outcome = ProjectionOutcome::default();
                outcome.record_evidence_error(error);
                (BTreeMap::new(), outcome)
            }
        };

        let (cross, cross_outcome) = if has_flow {
            session.collect_cross(&plan.flow_matchers, plan.rule_capacity)
        } else {
            Default::default()
        };
        outcome.record_cross(&cross_outcome);
        let mut projections = projections;
        for (module, evidence) in cross {
            if let Some(projection) = projections.get_mut(&module)
                && let Err(error) = projection.projected.merge_equal_capacity(evidence)
            {
                outcome.record_evidence_error(error);
            }
        }
        let outcome = outcome.finish();

        (
            ProjectMatcherModel {
                identity: ProjectMatcherIdentity::next(),
                matchers,
                projections,
            },
            outcome,
        )
    }
}

impl ProjectMatcherModel<'_, '_> {
    fn selection(&self) -> &CompiledRuleSelection<'_> {
        &self.matchers
    }

    /// Return handles that can be used to query this model's evidence.
    pub(in crate::analysis) fn modules(
        &self,
    ) -> impl Iterator<Item = ProjectModuleHandle<'_>> + '_ {
        self.projections
            .values()
            .map(|projection| ProjectModuleHandle {
                module: projection.module,
                owner: self.identity,
            })
    }

    /// Return deterministic, deduplicated evidence for a selected rule.
    pub(in crate::analysis) fn evidence_for(
        &self,
        module: ProjectModuleHandle<'_>,
        rule_index: RuleIndex,
        evidence_limit: usize,
    ) -> Result<Vec<ClassificationEvidence>, EvidenceQueryError> {
        self.evidence_for_checked(module, rule_index, evidence_limit)
    }

    /// Report assembly intentionally treats invalid handles as omitted
    /// evidence after it has completed the checked query at this boundary.
    fn evidence_for_lossy(
        &self,
        module: ProjectModuleHandle<'_>,
        rule_index: RuleIndex,
        evidence_limit: usize,
    ) -> Vec<ClassificationEvidence> {
        self.evidence_for(module, rule_index, evidence_limit)
            .unwrap_or_default()
    }

    fn evidence_for_checked(
        &self,
        module: ProjectModuleHandle<'_>,
        rule_index: RuleIndex,
        evidence_limit: usize,
    ) -> Result<Vec<ClassificationEvidence>, EvidenceQueryError> {
        if !self.matchers.is_selected(rule_index) {
            return Err(EvidenceQueryError::UnselectedRule);
        }
        if module.owner != self.identity {
            return Err(EvidenceQueryError::ForeignModel);
        }
        let Some(matcher) = self.matchers.get(rule_index) else {
            return Err(EvidenceQueryError::UnknownRule);
        };
        let Some(projection) = self.projections.get(&module.id()) else {
            return Err(EvidenceQueryError::UnknownModule);
        };
        let mut evidence = projection.evidence_for(matcher, rule_index);

        crate::analysis::matching::evidence::normalize_evidence(&mut evidence, evidence_limit);
        Ok(evidence)
    }
}

#[cfg(test)]
mod tests;
