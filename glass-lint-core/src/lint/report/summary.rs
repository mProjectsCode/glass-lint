use std::collections::BTreeMap;

use crate::{
    REPORT_VERSION,
    analysis::{ProjectSemanticModel, project::projection::ProjectionOutcome},
    lint::report::ProjectReportSession,
    project::{AnalysisReport, Diagnostic, FileReport, ProjectRelativePath, ReportCompletion},
};

pub(super) fn assemble_project_report(
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    files: BTreeMap<ProjectRelativePath, FileReport>,
    diagnostics: Vec<Diagnostic>,
    outcome: &ProjectionOutcome,
) -> AnalysisReport {
    let files: Vec<FileReport> = files.into_values().collect();
    let aggregate = AnalysisReport::aggregate(&files, &diagnostics);

    let mut operations = project.operation_counts();
    operations.record_evidence(aggregate.evidence_steps());
    let metrics = outcome.metrics();
    operations.record_effect_projections(metrics.effect_projections());
    operations.record_path_metrics(
        metrics.max_live_alternatives(),
        session.trace_node_count(),
        metrics.trace_heads(),
        metrics.coalescing_comparisons(),
        metrics.fixed_point_iterations(),
        aggregate.rendered_traces(),
    );

    AnalysisReport::new(
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
    )
}
