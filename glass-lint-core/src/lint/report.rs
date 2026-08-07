use std::{collections::BTreeMap, time::Instant};

use crate::{
    AnalysisLimits, ParseDiagnostic,
    analysis::{ProjectSemanticModel, ResolvedLinkInput},
    api::classification::RuleIndex,
    lint::catalog::RuleCatalog,
    project::{AnalysisReport, ProjectRelativePath, SourceTable},
};

mod diagnostics;
mod evidence;
mod summary;

pub struct ProjectAnalysis {
    pub report: AnalysisReport,
    pub linking: std::time::Duration,
    pub matching: std::time::Duration,
}

pub struct ReportAssembly<'a> {
    pub(super) catalog: &'a RuleCatalog,
    enabled: &'a [RuleIndex],
    evidence_limit: usize,
}

impl<'a> ReportAssembly<'a> {
    pub fn new(catalog: &'a RuleCatalog, enabled: &'a [RuleIndex], evidence_limit: usize) -> Self {
        Self {
            catalog,
            enabled,
            evidence_limit,
        }
    }

    pub fn finish(
        &self,
        sources: &SourceTable,
        link_input: ResolvedLinkInput,
        parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
        limits: &AnalysisLimits,
    ) -> ProjectAnalysis {
        let (mut files, parse_failures) =
            diagnostics::initialize_project_files(sources, parse_diagnostics);

        let linking_start = Instant::now();
        let mut project = ProjectSemanticModel::link_with_limits(link_input, limits);
        for (path, failure) in parse_failures {
            project.record_parse_failure(path, failure);
        }
        let linking = linking_start.elapsed();
        let link_counts = project.operation_counts().finish();

        tracing::info!(
            target: "glass_lint::project::link",
            files = link_counts.files(),
            requests = link_counts.requests(),
            edges = link_counts.edges(),
            elapsed = ?linking,
            "stage finished"
        );

        let matching_start = Instant::now();
        let (classifications, projection_outcome) = project.classify_with_evidence_limit(
            self.catalog.compiled(),
            self.enabled,
            self.evidence_limit,
        );
        project.record_flow_exhaustion(&projection_outcome);
        let matching = matching_start.elapsed();

        evidence::populate_project_files(self, &project, &classifications, &mut files);
        let diagnostics = diagnostics::attach_project_diagnostics(&project, &mut files);
        let report =
            summary::assemble_project_report(&project, files, diagnostics, &projection_outcome);
        let report_summary = report.summary();

        tracing::info!(
            target: "glass_lint::project::matching",
            files = report.operations().files(),
            findings = report_summary.findings(),
            evidence = report.operations().evidence(),
            diagnostics = report.diagnostics().len() + report_summary.parse_diagnostics(),
            elapsed = ?matching,
            "stage finished"
        );

        ProjectAnalysis {
            report,
            linking,
            matching,
        }
    }
}
