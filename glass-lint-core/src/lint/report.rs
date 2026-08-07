use std::{collections::BTreeMap, time::Instant};

use crate::{
    AnalysisLimits, ParseDiagnostic,
    analysis::{
        AnalysisComponent, AnalysisStatus, IncompleteReason, ProjectSemanticModel,
        ResolvedLinkInput, StatusScope,
        project::projection::ProjectionOutcome,
        trace::{TraceArena, TraceNodeId, TraceStep},
    },
    api::classification::RuleIndex,
    lint::catalog::RuleCatalog,
    project::{AnalysisReport, ProjectRelativePath, SourceTable},
};

mod diagnostics;
mod evidence;
mod summary;

/// Result of linking and matching a resolved project, including phase timings.
pub struct ProjectAnalysis {
    report: AnalysisReport,
    linking: std::time::Duration,
    matching: std::time::Duration,
}

impl ProjectAnalysis {
    /// Consume the analysis result and return its assembled report.
    #[must_use]
    pub fn into_report(self) -> AnalysisReport {
        self.report
    }

    /// Return the time spent linking the resolved project.
    #[must_use]
    pub fn linking(&self) -> std::time::Duration {
        self.linking
    }

    /// Return the time spent matching the linked project.
    #[must_use]
    pub fn matching(&self) -> std::time::Duration {
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

    fn record_flow_exhaustion(
        &mut self,
        project: &ProjectSemanticModel,
        outcome: &ProjectionOutcome,
    ) {
        if outcome.status.effect_exhausted {
            for module_id in &outcome.status.effect_exhausted_modules {
                if let Some(module) = project.modules().find(|module| module.id() == *module_id) {
                    self.status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::BudgetExhausted {
                            component: AnalysisComponent::Effects,
                            limit: project.effect_limit(),
                            observed: outcome.status.effect_observed,
                        },
                    );
                }
            }
        }
        if outcome.status.local_exhausted || outcome.status.flow_exhausted {
            self.status.record(
                StatusScope::Project,
                IncompleteReason::BudgetExhausted {
                    component: AnalysisComponent::Flow,
                    limit: project.flow_limit(),
                    observed: outcome.status.flow_observed,
                },
            );
        }
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
        let (mut files, parse_failures) =
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

        let matching_start = Instant::now();
        let (classifications, projection_outcome, trace_arena) = project
            .classify_with_evidence_limit(
                self.catalog.compiled(),
                self.enabled,
                self.evidence_limit,
            );
        session.set_trace_arena(trace_arena);
        session.record_flow_exhaustion(&project, &projection_outcome);
        let matching = matching_start.elapsed();

        evidence::populate_project_files(self, &project, &session, &classifications, &mut files);
        let diagnostics = diagnostics::attach_project_diagnostics(&project, &session, &mut files);
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
            linking,
            matching,
        }
    }
}
