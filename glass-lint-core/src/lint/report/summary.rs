use crate::{
    REPORT_VERSION,
    analysis::{ProjectSemanticModel, project::projection::ProjectionOutcome},
    lint::report::ProjectReportSession,
    project::{AnalysisReport, Diagnostic, FileReport, ReportCompletion},
};

pub(super) fn assemble_project_report(
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    files: Vec<FileReport>,
    diagnostics: Vec<Diagnostic>,
    outcome: &ProjectionOutcome,
) -> AnalysisReport {
    let (evidence_steps, rendered_traces) = AnalysisReport::aggregate_and_evidence(&files);

    let mut operations = project.operation_counts();
    operations.record_evidence(evidence_steps);
    let metrics = outcome.metrics();
    operations.record_effect_projections(metrics.effect_projections());
    operations.record_max_live_alternatives(metrics.max_live_alternatives());
    operations.record_trace_nodes(session.trace_node_count());
    operations.record_trace_heads(metrics.trace_heads());
    operations.record_coalescing_comparisons(metrics.coalescing_comparisons());
    operations.record_fixed_point_iterations(metrics.fixed_point_iterations());
    operations.record_rendered_traces(rendered_traces);

    AnalysisReport::new(
        REPORT_VERSION,
        env!("CARGO_PKG_VERSION").into(),
        files,
        diagnostics,
        operations,
        if session.is_complete() {
            ReportCompletion::Complete
        } else {
            ReportCompletion::Partial
        },
    )
    .finalize()
}
