use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use crate::{
    AnalysisLimits, ParseDiagnostic,
    analysis::{
        AnalysisStatus, IncompleteReason, ProjectSemanticModel, ResolvedLinkInput, StatusScope,
        project::projection::ProjectionOutcome,
        trace::{TraceArena, TraceNodeId, TraceStep},
    },
    api::classification::{ClassificationResult, RuleIndex},
    lint::catalog::RuleCatalog,
    project::{AnalysisReport, ModuleId, ProjectRelativePath, SourceTable},
};

mod diagnostics;
mod evidence;
mod files;
mod summary;

use files::ReportFiles;

/// Result of linking and matching a resolved project, including phase timings.
pub struct ProjectAnalysis {
    report: AnalysisReport,
    timings: ProjectAnalysisTimings,
}

impl ProjectAnalysis {
    /// Consume the analysis into its report.
    #[must_use]
    pub fn into_report(self) -> AnalysisReport {
        self.report
    }

    /// Consume the result into its report and phase-timing values.
    #[must_use]
    pub fn into_parts(self) -> (AnalysisReport, ProjectAnalysisTimings) {
        (self.report, self.timings)
    }
}

/// Phase timings recorded while linking and matching one resolved project.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProjectAnalysisTimings {
    linking: Duration,
    matching: Duration,
}

impl ProjectAnalysisTimings {
    /// Return the time spent linking the resolved project.
    #[must_use]
    pub fn linking(&self) -> Duration {
        self.linking
    }

    /// Return the time spent matching the linked project.
    #[must_use]
    pub fn matching(&self) -> Duration {
        self.matching
    }
}

pub(super) struct ProjectReportSession {
    status: AnalysisStatus,
    trace_arena: Option<TraceArena>,
}

impl ProjectReportSession {
    fn new(project: &ProjectSemanticModel) -> Self {
        Self {
            status: project.status_snapshot(),
            trace_arena: None,
        }
    }

    fn record_parse_failure(
        &mut self,
        path: ProjectRelativePath,
        kind: crate::parse::ParseFailureKind,
    ) {
        self.status.record(
            StatusScope::File(path),
            IncompleteReason::ParseFailure { kind },
        );
    }

    fn record_projection_status(
        &mut self,
        project: &ProjectSemanticModel,
        outcome: &ProjectionOutcome,
    ) {
        outcome.record_analysis_status(project, &mut self.status);
    }

    fn record_rule_selection_failure(&mut self, error: impl std::fmt::Debug) {
        self.status.record(
            StatusScope::Project,
            IncompleteReason::RuleSelectionInvalid {
                reason: format!("{error:?}"),
            },
        );
    }

    fn set_trace_arena(&mut self, trace_arena: TraceArena) {
        self.trace_arena = Some(trace_arena);
    }

    pub(super) fn status_diagnostics(
        &self,
    ) -> (
        Vec<(ProjectRelativePath, crate::project::AnalysisDiagnostic)>,
        Vec<crate::project::AnalysisDiagnostic>,
    ) {
        self.status.diagnostics()
    }

    pub(super) fn is_complete(&self) -> bool {
        self.status.is_complete()
    }

    pub(super) fn reconstruct_trace(&self, head: TraceNodeId) -> Option<Vec<TraceStep>> {
        self.trace_arena
            .as_ref()
            .and_then(|arena| arena.reconstruct_trace(head))
    }

    pub(super) fn trace_node_count(&self) -> usize {
        self.trace_arena.as_ref().map_or(0, TraceArena::node_count)
    }
}

pub struct ProjectReportAssembler {
    project: ProjectSemanticModel,
    session: ProjectReportSession,
    files: ReportFiles,
    linking: Duration,
}

impl ProjectReportAssembler {
    pub fn link(
        sources: &SourceTable,
        link_input: ResolvedLinkInput,
        parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
        limits: &AnalysisLimits,
    ) -> Self {
        let (files, parse_failures) = ReportFiles::initialize(sources, parse_diagnostics);
        let linking_start = Instant::now();
        let project = ProjectSemanticModel::link_with_limits(link_input, limits);
        let mut session = ProjectReportSession::new(&project);
        for (path, failure) in parse_failures {
            session.record_parse_failure(path, failure);
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

        Self {
            project,
            session,
            files,
            linking,
        }
    }

    pub fn assemble(
        mut self,
        catalog: &RuleCatalog,
        enabled: &[RuleIndex],
        evidence_limit: usize,
    ) -> ProjectAnalysis {
        let (classifications, projection_outcome, matching) =
            self.match_project(catalog, enabled, evidence_limit);
        self.render_findings(catalog, &classifications);
        self.finish(&projection_outcome, matching)
    }

    fn match_project(
        &mut self,
        catalog: &RuleCatalog,
        enabled: &[RuleIndex],
        evidence_limit: usize,
    ) -> (
        BTreeMap<ModuleId, ClassificationResult>,
        ProjectionOutcome,
        Duration,
    ) {
        let matching_start = Instant::now();
        let (classifications, projection_outcome, trace_arena) = match self
            .project
            .classify_with_evidence_limit(catalog.compiled(), enabled, evidence_limit)
        {
            Ok(result) => result,
            Err(error) => {
                self.session.record_rule_selection_failure(error);
                (
                    BTreeMap::new(),
                    ProjectionOutcome::default(),
                    TraceArena::new(0),
                )
            }
        };
        self.session.set_trace_arena(trace_arena);
        self.session
            .record_projection_status(&self.project, &projection_outcome);
        let matching = matching_start.elapsed();
        (classifications, projection_outcome, matching)
    }

    fn render_findings(
        &mut self,
        catalog: &RuleCatalog,
        classifications: &BTreeMap<ModuleId, ClassificationResult>,
    ) {
        evidence::FindingRenderer::new(catalog, &self.project, &self.session)
            .populate_project_files(classifications, &mut self.files);
        diagnostics::attach_project_diagnostics(&self.project, &self.session, &mut self.files);
    }

    fn finish(self, projection_outcome: &ProjectionOutcome, matching: Duration) -> ProjectAnalysis {
        let Self {
            project,
            session,
            files,
            linking,
            ..
        } = self;
        let (files, diagnostics) = files.into_parts();
        let report = summary::assemble_project_report(
            &project,
            &session,
            files,
            diagnostics,
            projection_outcome,
        );
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
            timings: ProjectAnalysisTimings { linking, matching },
        }
    }
}
