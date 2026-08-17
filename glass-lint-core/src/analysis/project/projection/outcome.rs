use crate::{
    analysis::{
        ModuleId, ProjectSemanticModel, flow,
        flow::projector::LocalFlowProjectionOutcome,
        semantic::status::{AnalysisComponent, AnalysisStatus, IncompleteReason, StatusScope},
    },
    api::classification::RuleEvidenceError,
};

#[derive(Debug, Default)]
pub struct ProjectionOutcome {
    pub(in crate::analysis::project::projection) status: ProjectionStatus,
    pub(in crate::analysis::project::projection) metrics: ProjectionMetrics,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::project::projection) enum ProjectionCompletion {
    #[default]
    Complete,
    Incomplete,
}

impl ProjectionCompletion {
    pub(in crate::analysis::project::projection) fn is_incomplete(self) -> bool {
        matches!(self, Self::Incomplete)
    }

    pub(in crate::analysis::project::projection) fn mark_incomplete(&mut self) {
        *self = Self::Incomplete;
    }
}

#[derive(Debug, Default)]
pub(in crate::analysis::project::projection) struct ProjectionStatus {
    pub(in crate::analysis::project::projection) flow: ProjectionCompletion,
    /// Operation count when exhaustion was reached, if applicable.
    pub(in crate::analysis::project::projection) flow_observed: Option<usize>,
    /// Flow-owned operations, excluding matcher overlay construction.
    pub(in crate::analysis::project::projection) flow_operations: usize,
    pub(in crate::analysis::project::projection) effects: ProjectionCompletion,
    /// Effect operations consumed when the effect budget was exhausted.
    pub(in crate::analysis::project::projection) effect_observed: Option<usize>,
    /// Modules whose effect extraction was incomplete.
    pub(in crate::analysis::project::projection) effect_exhausted_modules: Vec<ModuleId>,
    pub(in crate::analysis::project::projection) evidence_error: Option<RuleEvidenceError>,
}

impl ProjectionStatus {
    fn record_analysis_status(&self, project: &ProjectSemanticModel, status: &mut AnalysisStatus) {
        if self.effects.is_incomplete() {
            for module_id in &self.effect_exhausted_modules {
                if let Some(module) = project.module(*module_id) {
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
                RuleEvidenceError::RuleOutOfRange { rule, capacity } => {
                    (capacity, rule.get().saturating_add(1))
                }
                RuleEvidenceError::CapacityMismatch { expected, actual } => (expected, actual),
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

    pub(super) fn record_effects(
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

    pub(super) fn record_evidence_error(&mut self, error: RuleEvidenceError) {
        self.status.evidence_error.get_or_insert(error);
    }

    pub(super) fn record_local(&mut self, local: &LocalFlowProjectionOutcome) {
        self.record_flow(local.is_exhausted(), local.operations);
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
    }

    pub(super) fn record_cross(&mut self, cross: &flow::cross::CrossProjectionOutcome) {
        self.record_flow(cross.completion.is_incomplete(), cross.operations);
        self.metrics.effect_projections = cross.projections;
        self.metrics.trace_heads = self.metrics.trace_heads.saturating_add(cross.trace_heads);
    }

    fn record_flow(&mut self, incomplete: bool, operations: usize) {
        if incomplete {
            self.status.flow.mark_incomplete();
        }
        self.status.flow_operations = self.status.flow_operations.saturating_add(operations);
    }

    pub(super) fn finish(mut self) -> Self {
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
