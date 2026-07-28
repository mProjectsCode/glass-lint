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
    overlay: LinkedOccurrenceView<'project>,
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
    /// Complete trace heads emitted by local and cross-module flow.
    pub trace_heads: usize,
    /// Maximum live local semantic alternatives.
    pub max_live_alternatives: usize,
    /// Local coalescing comparisons.
    pub coalescing_comparisons: usize,
    /// Local loop fixed-point iterations.
    pub fixed_point_iterations: usize,
}

impl ProjectSemanticModel {
    /// Project a linked semantic model into matcher queries without rewalking
    /// any source AST.  Side effects such as budget exhaustion and projection
    /// counts are returned in a `ProjectionOutcome` instead of being written
    /// back into `self`.
    pub fn project<'project, 'matchers>(
        &'project self,
        matchers: CompiledRuleSelection<'matchers>,
    ) -> (ProjectMatcherModel<'project, 'matchers>, ProjectionOutcome) {
        let plan = ProjectionPlan::from_selection(&matchers);
        let flow_limits = FlowLimits::from_flow_operations(self.flow_limit());
        let mut local_exhausted = false;
        let mut max_live_alternatives: usize = 0;
        let mut coalescing_comparisons: usize = 0;
        let mut fixed_point_iterations: usize = 0;
        let mut local_trace_heads: usize = 0;
        let mut local_operations: usize = 0;
        let mut session = LinkingSession::new(self.flow_limit());
        let mut arena = self.trace_arena.lock().unwrap();

        let projections: BTreeMap<ModuleId, ProjectModuleProjection<'project>> = self
            .modules
            .values()
            .map(|module| {
                let index = module.local().facts().matcher_index();
                let identities = self.module_identities(module.id(), &mut session);
                let result_identities = self.call_result_identities(module.id(), &mut session);
                let overlay = index.module_overlay(&identities);
                let (projected, local_outcome) = module.local().facts().project(
                    module.local().effects(),
                    &plan,
                    Some(&identities),
                    Some(&result_identities),
                    Some(&overlay),
                    flow_limits,
                    module.id(),
                    &mut arena,
                );
                if local_outcome.exhausted {
                    local_exhausted = true;
                }
                max_live_alternatives =
                    max_live_alternatives.max(local_outcome.max_live_alternatives);
                coalescing_comparisons =
                    coalescing_comparisons.saturating_add(local_outcome.coalescing_comparisons);
                fixed_point_iterations =
                    fixed_point_iterations.saturating_add(local_outcome.fixed_point_iterations);
                local_trace_heads = local_trace_heads.saturating_add(local_outcome.trace_heads);
                local_operations = local_operations.saturating_add(local_outcome.operations);
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

        let (cross, cross_outcome) =
            { flow::cross::collect(self, &matchers, &mut session, &mut arena) };
        let exhausted = local_exhausted || cross_outcome.exhausted;
        let flow_operations = local_operations.saturating_add(cross_outcome.operations);
        let outcome = ProjectionOutcome {
            flow_exhausted: exhausted,
            effect_projections: cross_outcome.projections,
            flow_observed: exhausted.then_some(flow_operations),
            local_exhausted,
            trace_heads: local_trace_heads.saturating_add(cross_outcome.trace_heads),
            max_live_alternatives,
            coalescing_comparisons,
            fixed_point_iterations,
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

    /// Record flow exhaustion status from a projection outcome.
    pub(crate) fn record_flow_exhaustion(&mut self, outcome: &ProjectionOutcome) {
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
                    Some(&projection.overlay),
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
