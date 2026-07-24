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
    pub fn new(
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

    pub fn into_parts(
        self,
    ) -> (
        u32,
        String,
        Vec<FileReport>,
        Vec<Diagnostic>,
        AnalysisOperationCounts,
        ReportCompletion,
    ) {
        (
            self.schema_version,
            self.tool_version,
            self.files,
            self.diagnostics,
            self.operations,
            self.completion,
        )
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
        self
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
        self.completion = ReportCompletion::Partial;
        self
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
