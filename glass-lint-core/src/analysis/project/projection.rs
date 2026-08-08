//! Compiled matcher overlays and project-level matcher evidence.
//!
//! Projection is deliberately after local fact construction and project
//! linking. It applies qualified identities once, composes bounded flow, and
//! leaves rule selection to the compiled matcher catalog.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    analysis::{
        ModuleId, ProjectModule, ProjectSemanticModel,
        facts::SemanticFacts,
        flow::{
            self,
            projector::{self as object_flow, FlowProjectionRule, LocalFlowProjectionOutcome},
        },
        lowering::status::{AnalysisComponent, AnalysisStatus, IncompleteReason, StatusScope},
        matching::{MatcherOverlayPolicy, MatcherProjectContext, MatcherProjectInputs},
        model::{flow::FlowLimits, value::ValueId},
        project::state::LinkingSession,
        trace::TraceArena,
    },
    api::{
        classification::{
            ClassificationEvidence, ClassificationResult, MatchedCapability, RuleEvidenceCapacity,
            RuleEvidenceError, RuleEvidenceTable, RuleIndex,
        },
        compiler::{
            CompiledMatcherPlan, CompiledRuleRecord, CompiledRuleSelection,
            object_flow::CompiledObjectFlow, physical::PhysicalRoot,
            requirements::FlowRequirements,
        },
    },
};

pub fn project_for_classification<'project, 'matchers>(
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

pub fn assemble_classification_results(
    matcher_catalog: &ProjectMatcherModel<'_, '_>,
    records: &[CompiledRuleRecord],
    selected: &[RuleIndex],
    evidence_limit: usize,
) -> BTreeMap<ModuleId, ClassificationResult> {
    matcher_catalog
        .modules()
        .map(|module| {
            let mut result = ClassificationResult::default();
            for rule_index in selected {
                let index = rule_index.get();
                let Some(record) = records.get(index) else {
                    continue;
                };
                let evidence =
                    matcher_catalog.evidence_for_lossy(module, *rule_index, evidence_limit);
                if evidence.is_empty() {
                    continue;
                }

                result.push_capability(MatchedCapability::new(
                    *rule_index,
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
    constrained_roots: Vec<PlannedConstrainedRoot<'a>>,
    flow_matchers: Vec<PlannedFlow<'a>>,
    rule_capacity: RuleEvidenceCapacity,
    needs_module_identities: bool,
    needs_call_result_identities: bool,
    needs_overlay: bool,
    flow_requirements: FlowRequirements,
}

struct ProjectionSession<'project, 'plan, 'roots, 'arena> {
    project: &'project ProjectSemanticModel,
    plan: &'plan ProjectionPlan<'roots>,
    flow_limits: FlowLimits,
    arena: &'arena mut TraceArena,
    linking: LinkingSession,
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
            linking: LinkingSession::new(project.flow_limit()),
        }
    }

    fn collect_cross(
        &mut self,
        matchers: &CompiledRuleSelection<'_>,
    ) -> (
        BTreeMap<ModuleId, RuleEvidenceTable>,
        flow::cross::CrossProjectionOutcome,
    ) {
        flow::cross::collect(self.project, matchers, &mut self.linking, self.arena)
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
        let need_module_ids = self.plan.needs_module_identities() || self.plan.needs_overlay();
        let need_result_ids = self.plan.needs_call_result_identities();
        let mut outcome = ProjectionOutcome::default();
        let projections = self
            .project
            .modules()
            .map(|module| -> Result<_, RuleEvidenceError> {
                let identities = need_module_ids.then(|| {
                    self.project
                        .module_identities(module.id(), &mut self.linking)
                });
                let result_identities = need_result_ids.then(|| {
                    self.project
                        .call_result_identities(module.id(), &mut self.linking)
                });
                let overlay_policy = if self.plan.needs_overlay() {
                    MatcherOverlayPolicy::Enabled
                } else {
                    MatcherOverlayPolicy::Disabled
                };
                let project_inputs =
                    MatcherProjectInputs::new(identities.as_ref(), result_identities.as_ref());
                let (matcher_context, overlay_ops) = MatcherProjectContext::from_facts(
                    module.local().facts(),
                    project_inputs,
                    overlay_policy,
                );
                outcome.metrics.operations = outcome.metrics.operations.saturating_add(overlay_ops);
                let effects = self.plan.needs_flow().then(|| module.local().effects());
                if let Some(effects) = effects
                    && effects.is_available()
                    && effects.completion().is_incomplete()
                {
                    outcome.record_effects(module.id(), effects);
                }
                let (projected, local) = project_facts(
                    ProjectionInputs {
                        facts: module.local().facts(),
                        effects,
                        plan: self.plan,
                        flow_limits: self.flow_limits,
                        module_id: module.id(),
                        trace_arena: self.arena,
                    },
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

#[derive(Clone, Copy)]
struct PlannedConstrainedRoot<'a> {
    rule_index: RuleIndex,
    root: &'a PhysicalRoot,
}

#[derive(Clone, Copy)]
struct PlannedFlow<'a> {
    rule_index: RuleIndex,
    root: PlannedLifecycleRoot<'a>,
}

#[derive(Clone, Copy)]
struct PhysicalRootIndex(usize);

impl PhysicalRootIndex {
    fn new(index: usize) -> Self {
        Self(index)
    }

    fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy)]
struct PlannedLifecycleRoot<'a> {
    index: PhysicalRootIndex,
    flow: &'a CompiledObjectFlow,
}

impl<'a> PlannedLifecycleRoot<'a> {
    fn from_physical(index: usize, root: &'a PhysicalRoot) -> Option<Self> {
        let PhysicalRoot::Lifecycle { flow } = root else {
            return None;
        };
        Some(Self {
            index: PhysicalRootIndex::new(index),
            flow,
        })
    }
}

impl<'a> PlannedConstrainedRoot<'a> {
    fn matcher_input(self) -> (usize, &'a PhysicalRoot) {
        (self.rule_index.get(), self.root)
    }
}

impl<'a> PlannedFlow<'a> {
    fn flow_input(self) -> FlowProjectionRule<'a> {
        FlowProjectionRule::new(self.rule_index, self.root.index.get(), self.root.flow)
    }
}

impl<'a> ProjectionPlan<'a> {
    pub(in crate::analysis) fn needs_overlay(&self) -> bool {
        self.needs_overlay
    }

    pub(in crate::analysis) fn needs_module_identities(&self) -> bool {
        self.needs_module_identities
    }

    pub(in crate::analysis) fn needs_call_result_identities(&self) -> bool {
        self.needs_call_result_identities
    }

    pub(in crate::analysis) fn flow_requirements(&self) -> &FlowRequirements {
        &self.flow_requirements
    }

    pub(in crate::analysis) fn needs_flow(&self) -> bool {
        !self.flow_matchers.is_empty()
    }

    pub(in crate::analysis) fn from_selection(selection: &'a CompiledRuleSelection<'a>) -> Self {
        let mut constrained_roots = Vec::new();
        let mut flow_matchers = Vec::new();
        let mut needs_overall_overlay = false;
        let mut needs_overall_module_ids = false;
        let mut needs_overall_result_ids = false;
        let mut flow_local = false;
        let mut flow_cross_call = false;
        for (rule_index, matcher) in selection.selected_matchers() {
            for root in matcher.physical_roots() {
                if matches!(
                    root,
                    PhysicalRoot::ConstrainedScan { constraints, .. }
                        if !constraints.groups().is_empty()
                ) {
                    constrained_roots.push(PlannedConstrainedRoot { rule_index, root });
                }
            }
            for (flow_index, root) in matcher.physical_roots().iter().enumerate() {
                if let Some(root) = PlannedLifecycleRoot::from_physical(flow_index, root) {
                    flow_matchers.push(PlannedFlow { rule_index, root });
                }
            }
            needs_overall_overlay = needs_overall_overlay || matcher.needs_project_overlay();
            needs_overall_module_ids =
                needs_overall_module_ids || matcher.needs_module_identities();
            needs_overall_result_ids =
                needs_overall_result_ids || matcher.needs_call_result_identities();
            let fr = matcher.flow_requirements();
            flow_local = flow_local || fr.local();
            flow_cross_call = flow_cross_call || fr.cross_call();
        }
        Self {
            constrained_roots,
            flow_matchers,
            rule_capacity: selection.evidence_capacity(),
            needs_module_identities: needs_overall_module_ids,
            needs_call_result_identities: needs_overall_result_ids,
            needs_overlay: needs_overall_overlay,
            flow_requirements: FlowRequirements::new(flow_local, flow_cross_call),
        }
    }
}

/// Execute the query-selected local matching and flow projection for one
/// immutable facts artifact.
struct ProjectionInputs<'a> {
    facts: &'a SemanticFacts,
    effects: Option<&'a crate::analysis::flow::effect::FunctionEffects>,
    plan: &'a ProjectionPlan<'a>,
    flow_limits: FlowLimits,
    module_id: ModuleId,
    trace_arena: &'a mut TraceArena,
}

fn project_facts(
    inputs: ProjectionInputs<'_>,
    matcher_context: &MatcherProjectContext<'_, '_>,
) -> Result<(RuleEvidenceTable, LocalFlowProjectionOutcome), RuleEvidenceError> {
    let ProjectionInputs {
        facts,
        effects,
        plan,
        flow_limits,
        module_id,
        trace_arena,
    } = inputs;
    let mut projected_evidence = RuleEvidenceTable::new(plan.rule_capacity);
    if !facts.stream().is_valid() || facts.values().get(ValueId::UNKNOWN).is_none() {
        return Ok((projected_evidence, LocalFlowProjectionOutcome::default()));
    }
    let constrained_roots = plan
        .constrained_roots
        .iter()
        .copied()
        .map(PlannedConstrainedRoot::matcher_input)
        .collect::<Vec<_>>();
    crate::analysis::matching::try_compute_constrained_evidence(
        matcher_context.artifact(),
        &constrained_roots,
        &mut projected_evidence,
        matcher_context.project(),
    )?;
    if plan.flow_matchers.is_empty() {
        return Ok((projected_evidence, LocalFlowProjectionOutcome::default()));
    }
    let Some(effects) = effects else {
        return Ok((projected_evidence, LocalFlowProjectionOutcome::default()));
    };
    if !effects.is_available() {
        return Ok((projected_evidence, LocalFlowProjectionOutcome::default()));
    }
    let flow_matchers = plan
        .flow_matchers
        .iter()
        .copied()
        .map(PlannedFlow::flow_input)
        .collect::<Vec<_>>();
    let outcome = object_flow::collect_into(
        facts.stream(),
        effects,
        &flow_matchers,
        &mut projected_evidence,
        flow_limits,
        module_id,
        trace_arena,
    );
    Ok((projected_evidence, outcome))
}

#[derive(Debug)]
/// Matcher-independent facts and cross-file evidence for one linked project.
pub struct ProjectMatcherModel<'project, 'matchers> {
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
pub struct ProjectModuleHandle<'project> {
    module: &'project ProjectModule,
    owner: ProjectMatcherIdentity,
}

impl ProjectModuleHandle<'_> {
    pub fn id(self) -> ModuleId {
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
#[derive(Debug, Default)]
pub struct ProjectionOutcome {
    status: ProjectionStatus,
    metrics: ProjectionMetrics,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ProjectionCompletion {
    #[default]
    Complete,
    Incomplete,
}

impl ProjectionCompletion {
    fn is_incomplete(self) -> bool {
        matches!(self, Self::Incomplete)
    }

    fn mark_incomplete(&mut self) {
        *self = Self::Incomplete;
    }
}

#[derive(Debug, Default)]
struct ProjectionStatus {
    flow: ProjectionCompletion,
    /// Operation count when exhaustion was reached, if applicable.
    flow_observed: Option<usize>,
    /// Flow-owned operations, excluding matcher overlay construction.
    flow_operations: usize,
    effects: ProjectionCompletion,
    /// Effect operations consumed when the effect budget was exhausted.
    effect_observed: Option<usize>,
    /// Modules whose effect extraction was incomplete.
    effect_exhausted_modules: Vec<ModuleId>,
    evidence_error: Option<RuleEvidenceError>,
}

impl ProjectionStatus {
    fn record_analysis_status(&self, project: &ProjectSemanticModel, status: &mut AnalysisStatus) {
        if self.effects.is_incomplete() {
            for module_id in &self.effect_exhausted_modules {
                if let Some(module) = project.modules().find(|module| module.id() == *module_id) {
                    status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::BudgetExhausted {
                            component: AnalysisComponent::Effects,
                            limit: project.effect_limit(),
                            observed: self.effect_observed,
                        },
                    );
                }
            }
        }
        if self.flow.is_incomplete() {
            status.record(
                StatusScope::Project,
                IncompleteReason::BudgetExhausted {
                    component: AnalysisComponent::Flow,
                    limit: project.flow_limit(),
                    observed: self.flow_observed,
                },
            );
        }
        if let Some(error) = self.evidence_error {
            let (expected, actual) = match error {
                RuleEvidenceError::CapacityMismatch { expected, actual } => (expected, actual),
                RuleEvidenceError::RuleOutOfRange { rule, capacity } => {
                    (capacity, rule.get().saturating_add(1))
                }
            };
            status.record(
                StatusScope::Project,
                IncompleteReason::EvidenceCapacityMismatch { expected, actual },
            );
        }
    }
}

#[derive(Debug, Default)]
pub struct ProjectionMetrics {
    /// Number of effect projections performed during this projection.
    effect_projections: usize,
    /// Complete trace heads emitted by local and cross-module flow.
    trace_heads: usize,
    /// Maximum live local semantic alternatives.
    max_live_alternatives: usize,
    /// Local coalescing comparisons.
    coalescing_comparisons: usize,
    /// Local loop fixed-point iterations.
    fixed_point_iterations: usize,
    operations: usize,
}

impl ProjectionOutcome {
    pub(crate) fn record_analysis_status(
        &self,
        project: &ProjectSemanticModel,
        status: &mut AnalysisStatus,
    ) {
        self.status.record_analysis_status(project, status);
    }

    pub(crate) fn metrics(&self) -> &ProjectionMetrics {
        &self.metrics
    }

    fn record_effects(
        &mut self,
        module: ModuleId,
        effects: &crate::analysis::flow::effect::FunctionEffects,
    ) {
        if !effects.completion().is_incomplete() {
            return;
        }
        self.status.effects.mark_incomplete();
        self.status.effect_exhausted_modules.push(module);
        self.status.effect_observed = Some(
            self.status
                .effect_observed
                .unwrap_or_default()
                .saturating_add(effects.operation_count()),
        );
    }

    fn record_evidence_error(&mut self, error: RuleEvidenceError) {
        self.status.evidence_error.get_or_insert(error);
    }

    fn record_local(&mut self, local: &LocalFlowProjectionOutcome) {
        if local.is_exhausted() {
            self.status.flow.mark_incomplete();
        }
        self.status.flow_operations = self.status.flow_operations.saturating_add(local.operations);
        self.metrics.max_live_alternatives = self
            .metrics
            .max_live_alternatives
            .max(local.max_live_alternatives);
        self.metrics.coalescing_comparisons = self
            .metrics
            .coalescing_comparisons
            .saturating_add(local.coalescing_comparisons);
        self.metrics.fixed_point_iterations = self
            .metrics
            .fixed_point_iterations
            .saturating_add(local.fixed_point_iterations);
        self.metrics.trace_heads = self.metrics.trace_heads.saturating_add(local.trace_heads);
        self.metrics.operations = self.metrics.operations.saturating_add(local.operations);
    }

    fn record_cross(&mut self, cross: &flow::cross::CrossProjectionOutcome) {
        if cross.completion.is_incomplete() {
            self.status.flow.mark_incomplete();
        }
        self.status.flow_operations = self.status.flow_operations.saturating_add(cross.operations);
        self.metrics.effect_projections = cross.projections;
        self.metrics.trace_heads = self.metrics.trace_heads.saturating_add(cross.trace_heads);
        self.metrics.operations = self.metrics.operations.saturating_add(cross.operations);
    }

    fn finish(mut self) -> Self {
        self.status.flow_observed = self
            .status
            .flow
            .is_incomplete()
            .then_some(self.status.flow_operations);
        self
    }
}

impl ProjectionMetrics {
    pub(crate) fn effect_projections(&self) -> usize {
        self.effect_projections
    }

    pub(crate) fn trace_heads(&self) -> usize {
        self.trace_heads
    }

    pub(crate) fn max_live_alternatives(&self) -> usize {
        self.max_live_alternatives
    }

    pub(crate) fn coalescing_comparisons(&self) -> usize {
        self.coalescing_comparisons
    }

    pub(crate) fn fixed_point_iterations(&self) -> usize {
        self.fixed_point_iterations
    }
}

impl ProjectSemanticModel {
    /// Project a linked semantic model into matcher queries without rewalking
    /// any source AST.  Side effects such as budget exhaustion and projection
    /// counts are returned in a `ProjectionOutcome` instead of being written
    /// back into `self`.
    #[cfg(test)]
    pub fn project<'project, 'matchers>(
        &'project self,
        matchers: CompiledRuleSelection<'matchers>,
    ) -> (ProjectMatcherModel<'project, 'matchers>, ProjectionOutcome) {
        let mut arena = TraceArena::new(self.trace_limit());
        self.project_with_arena(matchers, &mut arena)
    }

    pub(crate) fn project_with_arena<'project, 'matchers>(
        &'project self,
        matchers: CompiledRuleSelection<'matchers>,
        arena: &mut TraceArena,
    ) -> (ProjectMatcherModel<'project, 'matchers>, ProjectionOutcome) {
        let plan = ProjectionPlan::from_selection(&matchers);
        let flow_limits = FlowLimits::from_flow_operations(self.flow_limit());
        let has_flow = plan.flow_requirements().local() || plan.flow_requirements().cross_call();
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
            session.collect_cross(&matchers)
        } else {
            Default::default()
        };
        outcome.record_cross(&cross_outcome);
        let outcome = outcome.finish();

        let mut projections = projections;
        for (module, evidence) in cross {
            if let Some(projection) = projections.get_mut(&module) {
                projection
                    .projected
                    .merge(evidence)
                    .expect("projected evidence uses one catalog capacity");
            }
        }

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
    /// Return handles that can be used to query this model's evidence.
    pub fn modules(&self) -> impl Iterator<Item = ProjectModuleHandle<'_>> + '_ {
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
mod tests {
    use super::*;

    #[test]
    fn flow_observed_excludes_non_flow_projection_work() {
        let mut outcome = ProjectionOutcome::default();
        let mut local = LocalFlowProjectionOutcome::default();
        local.operations = 7;
        outcome.record_local(&local);
        let cross = flow::cross::CrossProjectionOutcome {
            operations: 5,
            ..flow::cross::CrossProjectionOutcome::default()
        };
        outcome.record_cross(&cross);
        outcome.status.flow.mark_incomplete();
        outcome.metrics.operations = 100;

        let finished = outcome.finish();

        assert_eq!(finished.status.flow_observed, Some(12));
    }
}
