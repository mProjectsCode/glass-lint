use crate::project::types::{
    AnalysisDiagnostic, AnalysisOperationCounts, Diagnostic, DiagnosticCode, DiagnosticKind,
    FileReport,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum ReportCompletion {
    Complete,
    Partial,
}

impl ReportCompletion {
    /// Combine completion states for reports that are being aggregated.
    ///
    /// A partial input makes the aggregate partial because the aggregate
    /// cannot claim more coverage than all of its inputs provide.
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        if self.is_partial() || other.is_partial() {
            Self::Partial
        } else {
            Self::Complete
        }
    }

    #[must_use]
    pub const fn is_partial(self) -> bool {
        matches!(self, Self::Partial)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AnalysisReport {
    schema_version: u32,
    tool_version: String,
    files: Vec<FileReport>,
    diagnostics: Vec<Diagnostic>,
    operations: AnalysisOperationCounts,
    completion: ReportCompletion,
    #[cfg_attr(feature = "serde", serde(skip))]
    aggregate: FinalizedReportAggregate,
}

impl AnalysisReport {
    #[cfg(test)]
    pub(crate) fn new(
        schema_version: u32,
        tool_version: String,
        files: Vec<FileReport>,
        diagnostics: Vec<Diagnostic>,
        operations: AnalysisOperationCounts,
        completion: ReportCompletion,
    ) -> Self {
        let aggregate = Self::aggregate(&files, &diagnostics);
        Self::new_with_aggregate(
            schema_version,
            tool_version,
            files,
            diagnostics,
            operations,
            completion,
            aggregate,
        )
    }

    pub(crate) fn new_with_aggregate(
        schema_version: u32,
        tool_version: String,
        files: Vec<FileReport>,
        diagnostics: Vec<Diagnostic>,
        operations: AnalysisOperationCounts,
        completion: ReportCompletion,
        aggregate: FinalizedReportAggregate,
    ) -> Self {
        Self {
            schema_version,
            tool_version,
            files,
            diagnostics,
            operations,
            completion,
            aggregate,
        }
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn tool_version(&self) -> &str {
        &self.tool_version
    }

    pub fn files(&self) -> &[FileReport] {
        &self.files
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn operations(&self) -> AnalysisOperationCounts {
        self.operations
    }

    pub fn completion(&self) -> ReportCompletion {
        self.completion
    }

    /// Serialize this finalized report using the stable machine-readable
    /// schema owned by core.
    #[cfg(feature = "serde")]
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Append one validated report's contents losslessly. Callers validate
    /// schema and tool identity before merging.
    pub(crate) fn merge(mut self, other: Self) -> Self {
        self.files.extend(other.files);
        self.diagnostics.extend(other.diagnostics);
        self.operations += other.operations;
        self.completion = self.completion.join(other.completion);
        self
    }

    pub(crate) fn finalize(mut self) -> Self {
        self.files
            .sort_by(|left, right| left.ordering_key().cmp(right.ordering_key()));
        self.diagnostics
            .sort_by(|left, right| left.ordering_key().cmp(&right.ordering_key()));
        self.aggregate = Self::aggregate(&self.files, &self.diagnostics);
        self
    }

    #[must_use]
    pub fn with_project_diagnostics(
        mut self,
        code: &DiagnosticCode,
        messages: impl IntoIterator<Item = String>,
    ) -> Self {
        self.diagnostics.extend(messages.into_iter().map(|message| {
            Diagnostic::Project(AnalysisDiagnostic::new(code.clone(), message, None))
        }));
        self.finalize()
    }

    #[must_use]
    pub fn into_partial(mut self, reason: impl std::fmt::Display) -> Self {
        self.diagnostics
            .push(Diagnostic::Project(AnalysisDiagnostic::new(
                DiagnosticKind::IncompleteProject.into(),
                reason.to_string(),
                None,
            )));
        self.completion = self.completion.join(ReportCompletion::Partial);
        self.finalize()
    }

    pub(crate) fn aggregate(
        files: &[FileReport],
        diagnostics: &[Diagnostic],
    ) -> FinalizedReportAggregate {
        Self::aggregate_and_evidence(files, diagnostics).0
    }

    /// Compute the summary aggregate and the evidence metrics that the
    /// serialized operation counts own, in a single scan.
    pub(crate) fn aggregate_and_evidence(
        files: &[FileReport],
        diagnostics: &[Diagnostic],
    ) -> (FinalizedReportAggregate, usize, usize) {
        FinalizedReportAggregate::from_parts(files, diagnostics)
    }
}

#[cfg(test)]
mod tests;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnalysisReportSummary {
    files: usize,
    findings: usize,
    parse_diagnostics: usize,
    file_diagnostics: usize,
    report_diagnostics: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FinalizedReportAggregate {
    summary: AnalysisReportSummary,
}

impl FinalizedReportAggregate {
    /// Compute the report summary and the evidence metrics in one scan. The
    /// evidence metrics are recorded into the serialized operation counts
    /// rather than retained in the aggregate.
    fn from_parts(files: &[FileReport], diagnostics: &[Diagnostic]) -> (Self, usize, usize) {
        let mut summary = AnalysisReportSummary {
            files: files.len(),
            ..AnalysisReportSummary::default()
        };
        let mut evidence_steps = 0usize;
        let mut rendered_traces = 0usize;

        for file in files {
            summary.findings += file.findings().len();
            summary.parse_diagnostics += file
                .diagnostics()
                .iter()
                .filter(|diagnostic| matches!(diagnostic, Diagnostic::Parse { .. }))
                .count();
            summary.file_diagnostics += file
                .diagnostics()
                .iter()
                .filter(|diagnostic| matches!(diagnostic, Diagnostic::Project(_)))
                .count();
            for finding in file.findings() {
                evidence_steps += finding
                    .evidence()
                    .traces()
                    .iter()
                    .map(|trace| trace.steps().len())
                    .sum::<usize>();
                rendered_traces += finding.evidence().traces().len();
            }
        }
        summary.report_diagnostics = diagnostics
            .iter()
            .filter(|diagnostic| matches!(diagnostic, Diagnostic::Project(_)))
            .count();
        (Self { summary }, evidence_steps, rendered_traces)
    }

    pub(crate) const fn summary(self) -> AnalysisReportSummary {
        self.summary
    }
}

impl AnalysisReportSummary {
    pub fn files(&self) -> usize {
        self.files
    }

    pub fn findings(&self) -> usize {
        self.findings
    }

    pub fn parse_diagnostics(&self) -> usize {
        self.parse_diagnostics
    }

    pub fn file_diagnostics(&self) -> usize {
        self.file_diagnostics
    }

    pub fn report_diagnostics(&self) -> usize {
        self.report_diagnostics
    }
}

impl AnalysisReport {
    pub fn summary(&self) -> AnalysisReportSummary {
        self.aggregate.summary()
    }
}
