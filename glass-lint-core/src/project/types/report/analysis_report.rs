use crate::project::types::{
    AnalysisDiagnostic, AnalysisOperationCounts, Diagnostic, DiagnosticCode, FileReport,
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
}

impl AnalysisReport {
    pub(crate) fn new(
        schema_version: u32,
        tool_version: String,
        files: Vec<FileReport>,
        diagnostics: Vec<Diagnostic>,
        operations: AnalysisOperationCounts,
        completion: ReportCompletion,
    ) -> Self {
        Self {
            schema_version,
            tool_version,
            files,
            diagnostics,
            operations,
            completion,
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
        let code = DiagnosticCode::new("incomplete_project")
            .expect("incomplete_project is a valid diagnostic code");
        self.diagnostics
            .push(Diagnostic::Project(AnalysisDiagnostic::new(
                code,
                reason.to_string(),
                None,
            )));
        self.completion = self.completion.join(ReportCompletion::Partial);
        self.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::ReportCompletion;

    #[test]
    fn joining_completion_states_is_monotone() {
        use ReportCompletion::{Complete, Partial};

        assert_eq!(Complete.join(Complete), Complete);
        assert_eq!(Complete.join(Partial), Partial);
        assert_eq!(Partial.join(Complete), Partial);
        assert_eq!(Partial.join(Partial), Partial);
        assert!(!Complete.is_partial());
        assert!(Partial.is_partial());
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnalysisReportSummary {
    files: usize,
    findings: usize,
    parse_diagnostics: usize,
    file_diagnostics: usize,
    report_diagnostics: usize,
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
        AnalysisReportSummary {
            files: self.files.len(),
            findings: self.files.iter().map(|f| f.findings().len()).sum(),
            parse_diagnostics: self
                .files
                .iter()
                .flat_map(|f| f.diagnostics().iter())
                .filter(|d| matches!(d, Diagnostic::Parse { .. }))
                .count(),
            file_diagnostics: self
                .files
                .iter()
                .flat_map(|f| f.diagnostics().iter())
                .filter(|d| matches!(d, Diagnostic::Project(_)))
                .count(),
            report_diagnostics: self
                .diagnostics
                .iter()
                .filter(|d| matches!(d, Diagnostic::Project(_)))
                .count(),
        }
    }
}
