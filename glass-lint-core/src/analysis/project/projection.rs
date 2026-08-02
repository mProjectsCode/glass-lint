//! Compiled matcher overlays and project-level matcher evidence.
//!
//! Projection is deliberately after local fact construction and project
//! linking. It applies qualified identities once, composes bounded flow, and
//! leaves rule selection to the compiled matcher catalog.

use std::collections::BTreeMap;

use crate::{
    analysis::{
        ModuleId, ProjectModule, ProjectSemanticModel,
        facts::SemanticFacts,
        flow::{
            self,
            projector::{self as object_flow, LocalFlowProjectionOutcome},
        },
        lowering::status::{AnalysisComponent, IncompleteReason, StatusScope},
        matching::{LinkedOccurrenceView, OccurrenceIndexes},
        model::flow::FlowLimits,
        project::{model::ExportResolution, state::LinkingSession},
        trace::TraceArena,
        value::ValueId,
    },
    api::{
        classification::{ClassificationEvidence, RuleEvidenceTable, RuleIndex},
        compiler::{
            CompiledRuleSelection, object_flow::CompiledObjectFlow, physical::PhysicalRoot,
            requirements::FlowRequirements,
        },
    },
};

/// Flattened matcher requirements shared by all module projections in one
/// linked project. The plan belongs to projection orchestration rather than
/// to the immutable facts artifact.
pub(in crate::analysis) struct ProjectionPlan<'a> {
    constrained_roots: Vec<PlannedConstrainedRoot<'a>>,
    flow_matchers: Vec<PlannedFlow<'a>>,
    rule_count: usize,
    needs_module_identities: bool,
    needs_call_result_identities: bool,
    needs_overlay: bool,
    flow_requirements: FlowRequirements,
}

#[derive(Clone, Copy)]
struct PlannedConstrainedRoot<'a> {
    rule_index: RuleIndex,
    root: &'a PhysicalRoot,
}

#[derive(Clone, Copy)]
struct PlannedFlow<'a> {
    rule_index: RuleIndex,
    root_index: PhysicalRootIndex,
    flow: &'a CompiledObjectFlow,
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

impl<'a> PlannedConstrainedRoot<'a> {
    fn matcher_input(self) -> (usize, &'a PhysicalRoot) {
        (self.rule_index.get(), self.root)
    }
}

impl<'a> PlannedFlow<'a> {
    fn flow_input(self) -> (RuleIndex, usize, &'a CompiledObjectFlow) {
        (self.rule_index, self.root_index.get(), self.flow)
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
        let constrained_roots = selection
            .selected_matchers()
            .flat_map(|(rule_index, matcher)| {
                let roots: Vec<PlannedConstrainedRoot<'a>> = matcher
                    .physical_roots()
                    .iter()
                    .filter(|root| matches!(root, PhysicalRoot::ConstrainedScan { constraints, .. } if !constraints.groups().is_empty()))
                    .map(move |root| PlannedConstrainedRoot { rule_index, root })
                    .collect();
                roots
            })
            .collect::<Vec<_>>();

        let mut needs_overall_overlay = false;
        let mut needs_overall_module_ids = false;
        let mut needs_overall_result_ids = false;
        let mut flow_local = false;
        let mut flow_cross_call = false;
        let mut flow_cross_file = false;
        for (_, matcher) in selection.selected_matchers() {
            needs_overall_overlay = needs_overall_overlay || matcher.needs_project_overlay();
            needs_overall_module_ids =
                needs_overall_module_ids || matcher.needs_module_identities();
            needs_overall_result_ids =
                needs_overall_result_ids || matcher.needs_call_result_identities();
            let fr = matcher.flow_requirements();
            flow_local = flow_local || fr.local;
            flow_cross_call = flow_cross_call || fr.cross_call;
            flow_cross_file = flow_cross_file || fr.cross_file;
        }

        let flow_matchers =
            selection
                .selected_matchers()
                .flat_map(|(rule_index, matcher)| {
                    let ri = rule_index;
                    matcher.physical_roots().iter().enumerate().filter_map(
                        move |(flow_index, root)| {
                            if let PhysicalRoot::Lifecycle { flow } = root {
                                Some(PlannedFlow {
                                    rule_index: ri,
                                    root_index: PhysicalRootIndex::new(flow_index),
                                    flow,
                                })
                            } else {
                                None
                            }
                        },
                    )
                })
                .collect::<Vec<_>>();
        Self {
            constrained_roots,
            flow_matchers,
            rule_count: selection.rule_capacity(),
            needs_module_identities: needs_overall_module_ids,
            needs_call_result_identities: needs_overall_result_ids,
            needs_overlay: needs_overall_overlay,
            flow_requirements: FlowRequirements {
                local: flow_local,
                cross_call: flow_cross_call,
                cross_file: flow_cross_file,
            },
        }
    }
}

/// Execute the query-selected local matching and flow projection for one
/// immutable facts artifact.
struct ProjectionInputs<'a> {
    facts: &'a SemanticFacts,
    effects: Option<&'a crate::analysis::flow::effect::FunctionEffects>,
    plan: &'a ProjectionPlan<'a>,
    identities: Option<&'a crate::analysis::matching::ModuleIdentityMap>,
    result_identities: Option<&'a BTreeMap<ValueId, ExportResolution>>,
    overlay: Option<&'a LinkedOccurrenceView<'a>>,
    flow_limits: FlowLimits,
    module_id: ModuleId,
    trace_arena: &'a mut TraceArena,
}

fn project_facts(inputs: ProjectionInputs<'_>) -> (RuleEvidenceTable, LocalFlowProjectionOutcome) {
    let ProjectionInputs {
        facts,
        effects,
        plan,
        identities,
        result_identities,
        overlay,
        flow_limits,
        module_id,
        trace_arena,
    } = inputs;
    let mut projected_evidence = RuleEvidenceTable::new(plan.rule_count);
    if !facts.stream().is_valid() || facts.values().get(ValueId::UNKNOWN).is_none() {
        return (projected_evidence, LocalFlowProjectionOutcome::default());
    }
    let constrained_roots = plan
        .constrained_roots
        .iter()
        .copied()
        .map(PlannedConstrainedRoot::matcher_input)
        .collect::<Vec<_>>();
    crate::analysis::matching::compute_constrained_evidence_from_stream_with_overlay(
        facts.stream(),
        facts.matcher_index(),
        &constrained_roots,
        &mut projected_evidence,
        overlay,
        identities,
        result_identities,
    );
    if plan.flow_matchers.is_empty() {
        return (projected_evidence, LocalFlowProjectionOutcome::default());
    }
    let Some(effects) = effects else {
        return (projected_evidence, LocalFlowProjectionOutcome::default());
    };
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
    (projected_evidence, outcome)
}

#[derive(Debug)]
/// Matcher-independent facts and cross-file evidence for one linked project.
pub struct ProjectMatcherModel<'project, 'matchers> {
    matchers: CompiledRuleSelection<'matchers>,
    projections: BTreeMap<ModuleId, ProjectModuleProjection<'project>>,
}

#[derive(Debug)]
struct ProjectModuleProjection<'project> {
    index: &'project OccurrenceIndexes,
    overlay: Option<LinkedOccurrenceView<'project>>,
    projected: RuleEvidenceTable,
}

/// Side effects produced by a projection that were previously written back
/// into the project model through hidden interior mutability.  The caller
/// decides how to merge or report these instead of the project mutating
/// itself through a shared reference.
#[derive(Debug, Default)]
pub struct ProjectionOutcome {
    /// Whether local flow projection exhausted its budget in any module.
    pub local_exhausted: bool,
    /// Whether cross-module flow projection exhausted its budget.
    pub flow_exhausted: bool,
    /// Number of effect projections performed during this projection.
    pub effect_projections: usize,
    /// Operation count when exhaustion was reached, if applicable.
    pub flow_observed: Option<usize>,
    /// Whether lazy function-effect extraction reached its budget.
    pub effect_exhausted: bool,
    /// Effect operations consumed when the effect budget was exhausted.
    pub effect_observed: Option<usize>,
    /// Modules whose effect extraction was incomplete.
    effect_exhausted_modules: Vec<ModuleId>,
    /// Complete trace heads emitted by local and cross-module flow.
    pub trace_heads: usize,
    /// Maximum live local semantic alternatives.
    pub max_live_alternatives: usize,
    /// Local coalescing comparisons.
    pub coalescing_comparisons: usize,
    /// Local loop fixed-point iterations.
    pub fixed_point_iterations: usize,
    operations: usize,
}

impl ProjectionOutcome {
    fn record_effects(
        &mut self,
        module: ModuleId,
        effects: &crate::analysis::flow::effect::FunctionEffects,
    ) {
        if !effects.budget_exhausted() {
            return;
        }
        self.effect_exhausted = true;
        self.effect_exhausted_modules.push(module);
        self.effect_observed = Some(
            self.effect_observed
                .unwrap_or_default()
                .saturating_add(effects.operation_count()),
        );
    }

    fn record_local(&mut self, local: &LocalFlowProjectionOutcome) {
        self.local_exhausted |= local.exhausted;
        self.flow_exhausted |= local.exhausted;
        self.max_live_alternatives = self.max_live_alternatives.max(local.max_live_alternatives);
        self.coalescing_comparisons = self
            .coalescing_comparisons
            .saturating_add(local.coalescing_comparisons);
        self.fixed_point_iterations = self
            .fixed_point_iterations
            .saturating_add(local.fixed_point_iterations);
        self.trace_heads = self.trace_heads.saturating_add(local.trace_heads);
        self.operations = self.operations.saturating_add(local.operations);
    }

    fn record_cross(&mut self, cross: &flow::cross::CrossProjectionOutcome) {
        self.flow_exhausted |= cross.exhausted;
        self.effect_projections = cross.projections;
        self.trace_heads = self.trace_heads.saturating_add(cross.trace_heads);
        self.operations = self.operations.saturating_add(cross.operations);
    }

    fn finish(mut self) -> Self {
        self.flow_observed = self.flow_exhausted.then_some(self.operations);
        self
    }
}

impl ProjectSemanticModel {
    /// Project a linked semantic model into matcher queries without rewalking
    /// any source AST.  Side effects such as budget exhaustion and projection
    /// counts are returned in a `ProjectionOutcome` instead of being written
    /// back into `self`.
    #[allow(clippy::too_many_lines)]
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
        let mut session = LinkingSession::new(self.flow_limit());

        let has_flow = plan.flow_requirements().local
            || plan.flow_requirements().cross_call
            || plan.flow_requirements().cross_file;
        let (projections, mut outcome) =
            self.project_modules(&plan, flow_limits, arena, &mut session);

        let (cross, cross_outcome) = if has_flow {
            flow::cross::collect(self, &matchers, &mut session, arena)
        } else {
            Default::default()
        };
        outcome.record_cross(&cross_outcome);
        let outcome = outcome.finish();

        let mut projections = projections;
        for (module, evidence) in cross {
            if let Some(projection) = projections.get_mut(&module) {
                projection.projected.merge(evidence);
            }
        }

        (
            ProjectMatcherModel {
                matchers,
                projections,
            },
            outcome,
        )
    }

    fn project_modules<'project>(
        &'project self,
        plan: &ProjectionPlan<'_>,
        flow_limits: FlowLimits,
        arena: &mut TraceArena,
        session: &mut LinkingSession,
    ) -> (
        BTreeMap<ModuleId, ProjectModuleProjection<'project>>,
        ProjectionOutcome,
    ) {
        let need_module_ids = plan.needs_module_identities() || plan.needs_overlay();
        let need_result_ids = plan.needs_call_result_identities();
        let mut outcome = ProjectionOutcome::default();
        let projections = self
            .modules
            .values()
            .map(|module| {
                let index = module.local().facts().matcher_index();
                let identities =
                    need_module_ids.then(|| self.module_identities(module.id(), session));
                let result_identities =
                    need_result_ids.then(|| self.call_result_identities(module.id(), session));
                let (overlay, overlay_ops) = if plan.needs_overlay() {
                    identities.as_ref().map_or((None, 0), |ids| {
                        let (view, ops) = index.module_overlay(ids);
                        (Some(view), ops)
                    })
                } else {
                    (None, 0)
                };
                outcome.operations = outcome.operations.saturating_add(overlay_ops);
                let effects = plan.needs_flow().then(|| module.local().effects());
                if let Some(effects) = effects
                    && effects.budget_exhausted()
                {
                    outcome.record_effects(module.id(), effects);
                }
                let (projected, local) = project_facts(ProjectionInputs {
                    facts: module.local().facts(),
                    effects,
                    plan,
                    identities: identities.as_ref(),
                    result_identities: result_identities.as_ref(),
                    overlay: overlay.as_ref(),
                    flow_limits,
                    module_id: module.id(),
                    trace_arena: arena,
                });
                outcome.record_local(&local);
                (
                    module.id(),
                    ProjectModuleProjection {
                        index,
                        overlay,
                        projected,
                    },
                )
            })
            .collect();
        (projections, outcome)
    }

    /// Record flow exhaustion status from a projection outcome.
    pub(crate) fn record_flow_exhaustion(&mut self, outcome: &ProjectionOutcome) {
        if outcome.effect_exhausted {
            for module in &outcome.effect_exhausted_modules {
                if let Some(module) = self.modules.get(module) {
                    self.status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::BudgetExhausted {
                            component: AnalysisComponent::Effects,
                            limit: self.effect_limit(),
                            observed: outcome.effect_observed,
                        },
                    );
                }
            }
        }
        if outcome.local_exhausted || outcome.flow_exhausted {
            self.status.record(
                StatusScope::Project,
                IncompleteReason::BudgetExhausted {
                    component: AnalysisComponent::Flow,
                    limit: self.flow_limit(),
                    observed: outcome.flow_observed,
                },
            );
        }
    }
}

impl ProjectMatcherModel<'_, '_> {
    /// Return deterministic, deduplicated evidence for a selected rule.
    pub fn evidence_for(
        &self,
        module: &ProjectModule,
        rule_index: RuleIndex,
        evidence_limit: usize,
    ) -> Vec<ClassificationEvidence> {
        if !self.matchers.is_selected(rule_index) {
            return Vec::new();
        }
        let Some(matcher) = self.matchers.get(rule_index) else {
            return Vec::new();
        };
        let names = module.local().facts().names();
        let mut evidence = self
            .projections
            .get(&module.id())
            .map_or_else(Vec::new, |projection| {
                projection.index.evidence_for_with_overlay(
                    matcher,
                    projection.overlay.as_ref(),
                    names,
                )
            });

        if let Some(projected) = self
            .projections
            .get(&module.id())
            .and_then(|projection| projection.projected.for_rule(rule_index))
        {
            evidence.extend_from_slice(projected);
        }

        crate::analysis::matching::evidence::normalize_evidence(&mut evidence, evidence_limit);
        evidence
    }
}
