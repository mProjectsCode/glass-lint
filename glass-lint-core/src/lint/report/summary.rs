use crate::{
    REPORT_VERSION,
    analysis::{ProjectSemanticModel, project::projection::ProjectionOutcome},
    lint::report::ProjectReportSession,
    project::{AnalysisReport, Diagnostic, FileReport, ReportCompletion, types::ReportPathMetrics},
};

pub(super) fn assemble_project_report(
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    files: Vec<FileReport>,
    mut diagnostics: Vec<Diagnostic>,
    outcome: &ProjectionOutcome,
) -> AnalysisReport {
    diagnostics.sort_by(|left, right| left.code().cmp(right.code()));
    let (aggregate, evidence_steps, rendered_traces) =
        AnalysisReport::aggregate_and_evidence(&files, &diagnostics);

    let mut operations = project.operation_counts();
    operations.record_evidence(evidence_steps);
    let metrics = outcome.metrics();
    operations.record_effect_projections(metrics.effect_projections());
    operations.record_path_metrics(ReportPathMetrics {
        max_live_alternatives: metrics.max_live_alternatives(),
        trace_nodes: session.trace_node_count(),
        trace_heads: metrics.trace_heads(),
        coalescing_comparisons: metrics.coalescing_comparisons(),
        fixed_point_iterations: metrics.fixed_point_iterations(),
        rendered_traces,
    });

    AnalysisReport::new_with_aggregate(
        REPORT_VERSION,
        env!("CARGO_PKG_VERSION").into(),
        files,
        diagnostics,
        operations.finish(),
        if session.is_complete() {
            ReportCompletion::Complete
        } else {
            ReportCompletion::Partial
        },
        aggregate,
    )
}
