use std::collections::BTreeMap;

use crate::{
    REPORT_VERSION,
    analysis::{ProjectSemanticModel, project::projection::ProjectionOutcome},
    project::{AnalysisReport, Diagnostic, FileReport, ProjectRelativePath, ReportCompletion},
};

pub(super) fn assemble_project_report(
    project: &ProjectSemanticModel,
    files: BTreeMap<ProjectRelativePath, FileReport>,
    diagnostics: Vec<Diagnostic>,
    outcome: &ProjectionOutcome,
) -> AnalysisReport {
    let evidence = files
        .values()
        .map(|file| {
            file.findings()
                .iter()
                .map(|finding| {
                    finding
                        .evidence()
                        .traces()
                        .iter()
                        .map(|trace| trace.steps().len())
                        .sum::<usize>()
                })
                .sum::<usize>()
        })
        .sum();
    let rendered_traces = files
        .values()
        .flat_map(FileReport::findings)
        .map(|finding| finding.evidence().traces().len())
        .sum();

    let mut operations = project.operation_counts();
    operations.record_evidence(evidence);
    let metrics = outcome.metrics();
    operations.record_effect_projections(metrics.effect_projections());
    operations.record_path_metrics(
        metrics.max_live_alternatives(),
        project.trace_node_count(),
        metrics.trace_heads(),
        metrics.coalescing_comparisons(),
        metrics.fixed_point_iterations(),
        rendered_traces,
    );

    AnalysisReport::new(
        REPORT_VERSION,
        env!("CARGO_PKG_VERSION").into(),
        files.into_values().collect(),
        diagnostics,
        operations.finish(),
        if project.is_complete() {
            ReportCompletion::Complete
        } else {
            ReportCompletion::Partial
        },
    )
}
