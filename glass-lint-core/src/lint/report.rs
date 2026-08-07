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
    project::{AnalysisReport, FileReport, ModuleId, ProjectRelativePath, SourceTable},
};

mod diagnostics;
mod evidence;
mod summary;

/// Result of linking and matching a resolved project, including phase timings.
pub struct ProjectAnalysis {
    report: AnalysisReport,
    timings: ProjectAnalysisTimings,
}

impl ProjectAnalysis {
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
    trace_arena: TraceArena,
}

impl ProjectReportSession {
    fn new(project: &ProjectSemanticModel, trace_limit: usize) -> Self {
        Self {
            status: project.status_snapshot(),
            trace_arena: TraceArena::new(trace_limit),
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

    fn set_trace_arena(&mut self, trace_arena: TraceArena) {
        self.trace_arena = trace_arena;
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
        self.trace_arena.reconstruct_trace(head)
    }

    pub(super) fn trace_node_count(&self) -> usize {
        self.trace_arena.node_count()
    }
}

pub(super) struct ReportAssembly<'a> {
    pub(super) catalog: &'a RuleCatalog,
    enabled: &'a [RuleIndex],
    evidence_limit: usize,
}

struct LinkedReport {
    project: ProjectSemanticModel,
    session: ProjectReportSession,
    files: BTreeMap<ProjectRelativePath, FileReport>,
    linking: Duration,
}

struct MatchedReport {
    project: ProjectSemanticModel,
    session: ProjectReportSession,
    files: BTreeMap<ProjectRelativePath, FileReport>,
    classifications: BTreeMap<ModuleId, ClassificationResult>,
    projection_outcome: ProjectionOutcome,
    linking: Duration,
    matching: Duration,
}

struct RenderedReport {
    project: ProjectSemanticModel,
    session: ProjectReportSession,
    files: BTreeMap<ProjectRelativePath, FileReport>,
    diagnostics: Vec<crate::project::Diagnostic>,
    projection_outcome: ProjectionOutcome,
    linking: Duration,
    matching: Duration,
}

impl LinkedReport {
    fn link(
        sources: &SourceTable,
        link_input: ResolvedLinkInput,
        parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
        limits: &AnalysisLimits,
    ) -> Self {
        let (files, parse_failures) =
            diagnostics::initialize_project_files(sources, parse_diagnostics);
        let linking_start = Instant::now();
        let project = ProjectSemanticModel::link_with_limits(link_input, limits);
        let mut session = ProjectReportSession::new(&project, limits.trace_nodes());
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

    fn match_project(self, assembly: &ReportAssembly<'_>) -> MatchedReport {
        let Self {
            project,
            mut session,
            files,
            linking,
        } = self;
        let matching_start = Instant::now();
        let (classifications, projection_outcome, trace_arena) = project
            .classify_with_evidence_limit(
                assembly.catalog.compiled(),
                assembly.enabled,
                assembly.evidence_limit,
            );
        session.set_trace_arena(trace_arena);
        session.record_projection_status(&project, &projection_outcome);
        let matching = matching_start.elapsed();

        MatchedReport {
            project,
            session,
            files,
            classifications,
            projection_outcome,
            linking,
            matching,
        }
    }
}

impl MatchedReport {
    fn render(self, assembly: &ReportAssembly<'_>) -> RenderedReport {
        let Self {
            project,
            session,
            files,
            classifications,
            projection_outcome,
            linking,
            matching,
        } = self;
        let mut files = files;
        evidence::populate_project_files(
            assembly,
            &project,
            &session,
            &classifications,
            &mut files,
        );
        let diagnostics = diagnostics::attach_project_diagnostics(&project, &session, &mut files);

        RenderedReport {
            project,
            session,
            files,
            diagnostics,
            projection_outcome,
            linking,
            matching,
        }
    }
}

impl RenderedReport {
    fn finish(self) -> ProjectAnalysis {
        let Self {
            project,
            session,
            files,
            diagnostics,
            projection_outcome,
            linking,
            matching,
        } = self;
        let report = summary::assemble_project_report(
            &project,
            &session,
            files,
            diagnostics,
            &projection_outcome,
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

impl<'a> ReportAssembly<'a> {
    pub(super) fn new(
        catalog: &'a RuleCatalog,
        enabled: &'a [RuleIndex],
        evidence_limit: usize,
    ) -> Self {
        Self {
            catalog,
            enabled,
            evidence_limit,
        }
    }

    pub(super) fn finish(
        &self,
        sources: &SourceTable,
        link_input: ResolvedLinkInput,
        parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
        limits: &AnalysisLimits,
    ) -> ProjectAnalysis {
        LinkedReport::link(sources, link_input, parse_diagnostics, limits)
            .match_project(self)
            .render(self)
            .finish()
    }
}
