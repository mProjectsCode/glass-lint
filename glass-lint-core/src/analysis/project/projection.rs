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
    constrained_roots: Vec<(usize, &'a PhysicalRoot)>,
    flow_matchers: Vec<(RuleIndex, usize, &'a CompiledObjectFlow)>,
    rule_count: usize,
    needs_module_identities: bool,
    needs_call_result_identities: bool,
    needs_overlay: bool,
    flow_requirements: FlowRequirements,
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
                let roots: Vec<(usize, &PhysicalRoot)> = matcher
                    .physical_roots()
                    .iter()
                    .filter(|root| matches!(root, PhysicalRoot::ConstrainedScan { constraints, .. } if !constraints.groups().is_empty()))
                    .map(move |root| (rule_index.get(), root))
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
                                Some((ri, flow_index, flow))
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
#[allow(clippy::too_many_arguments)]
pub(in crate::analysis) fn project_facts(
    facts: &SemanticFacts,
    effects: Option<&crate::analysis::flow::effect::FunctionEffects>,
    plan: &ProjectionPlan<'_>,
    identities: Option<&crate::analysis::matching::ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, ExportResolution>>,
    overlay: Option<&LinkedOccurrenceView<'_>>,
    flow_limits: FlowLimits,
    module_id: ModuleId,
    trace_arena: &mut TraceArena,
) -> (RuleEvidenceTable, LocalFlowProjectionOutcome) {
    let mut projected_evidence = RuleEvidenceTable::new(plan.rule_count);
    if !facts.stream().is_valid() || facts.values().get(ValueId::UNKNOWN).is_none() {
        return (projected_evidence, LocalFlowProjectionOutcome::default());
    }
    crate::analysis::matching::compute_constrained_evidence_from_stream_with_overlay(
        facts.stream(),
        facts.matcher_index(),
        &plan.constrained_roots,
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
    let outcome = object_flow::collect_into(
        facts.stream(),
        effects,
        &plan.flow_matchers,
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
}

#[derive(Default)]
struct LocalProjectionOutcome {
    exhausted: bool,
    effect_exhausted: bool,
    effect_observed: Option<usize>,
    effect_exhausted_modules: Vec<ModuleId>,
    max_live_alternatives: usize,
    coalescing_comparisons: usize,
    fixed_point_iterations: usize,
    trace_heads: usize,
    operations: usize,
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
        let (projections, local) = self.project_modules(&plan, flow_limits, arena, &mut session);

        let (cross, cross_outcome) = if has_flow {
            flow::cross::collect(self, &matchers, &mut session, arena)
        } else {
            Default::default()
        };
        let exhausted = local.exhausted || cross_outcome.exhausted;
        let flow_operations = local.operations.saturating_add(cross_outcome.operations);
        let outcome = ProjectionOutcome {
            flow_exhausted: exhausted,
            effect_projections: cross_outcome.projections,
            flow_observed: exhausted.then_some(flow_operations),
            effect_exhausted: local.effect_exhausted,
            effect_observed: local.effect_observed,
            effect_exhausted_modules: local.effect_exhausted_modules,
            local_exhausted: local.exhausted,
            trace_heads: local.trace_heads.saturating_add(cross_outcome.trace_heads),
            max_live_alternatives: local.max_live_alternatives,
            coalescing_comparisons: local.coalescing_comparisons,
            fixed_point_iterations: local.fixed_point_iterations,
        };

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
        LocalProjectionOutcome,
    ) {
        let need_module_ids = plan.needs_module_identities() || plan.needs_overlay();
        let need_result_ids = plan.needs_call_result_identities();
        let mut outcome = LocalProjectionOutcome::default();
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
                    outcome.effect_exhausted = true;
                    outcome.effect_exhausted_modules.push(module.id());
                    outcome.effect_observed = Some(
                        outcome
                            .effect_observed
                            .unwrap_or_default()
                            .saturating_add(effects.operation_count()),
                    );
                }
                let (projected, local) = project_facts(
                    module.local().facts(),
                    effects,
                    plan,
                    identities.as_ref(),
                    result_identities.as_ref(),
                    overlay.as_ref(),
                    flow_limits,
                    module.id(),
                    arena,
                );
                outcome.exhausted |= local.exhausted;
                outcome.max_live_alternatives = outcome
                    .max_live_alternatives
                    .max(local.max_live_alternatives);
                outcome.coalescing_comparisons = outcome
                    .coalescing_comparisons
                    .saturating_add(local.coalescing_comparisons);
                outcome.fixed_point_iterations = outcome
                    .fixed_point_iterations
                    .saturating_add(local.fixed_point_iterations);
                outcome.trace_heads = outcome.trace_heads.saturating_add(local.trace_heads);
                outcome.operations = outcome.operations.saturating_add(local.operations);
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
