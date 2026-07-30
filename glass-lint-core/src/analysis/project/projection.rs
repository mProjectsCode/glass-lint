//! Compiled matcher overlays and project-level matcher evidence.
//!
//! Projection is deliberately after local fact construction and project
//! linking. It applies qualified identities once, composes bounded flow, and
//! leaves rule selection to the compiled matcher catalog.

use std::collections::BTreeMap;

use crate::{
    analysis::{
        ModuleId, ProjectModule, ProjectSemanticModel,
        facts::ProjectionPlan,
        flow::{self},
        lowering::status::{AnalysisComponent, IncompleteReason, StatusScope},
        matching::{LinkedOccurrenceView, OccurrenceIndexes},
        model::flow::FlowLimits,
        project::state::LinkingSession,
        trace::TraceArena,
    },
    api::{
        classification::{ClassificationEvidence, RuleIndex},
        compiler::CompiledRuleSelection,
    },
};

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
    projected: Vec<Vec<ClassificationEvidence>>,
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
                for (rule, values) in evidence.into_iter().enumerate() {
                    projection.projected[rule].extend(values);
                }
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
                let (projected, local) = module.local().facts().project(
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
            .and_then(|projection| projection.projected.get(rule_index.get()))
        {
            evidence.extend_from_slice(projected);
        }

        crate::analysis::matching::evidence::normalize_evidence(&mut evidence, evidence_limit);
        evidence
    }
}
