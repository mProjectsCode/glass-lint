use glass_lint_datastructures::SourceRange;

use crate::{
    RuleId, Severity,
    project::{EvidenceList, types::ProjectRelativePath},
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct DiagnosticCode(String);

const MAX_DIAGNOSTIC_CODE_LEN: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticKind {
    AmbiguousStarExport,
    EffectsBudgetExhausted,
    FlowBudgetExhausted,
    LinkingBudgetExhausted,
    InvalidParserSpan,
    MissingImportedExport,
    OutsideProjectTarget,
    FactsBudgetExhausted,
    NameBudgetExhausted,
    ScopeShapeMismatch,
    SourceTooLarge,
    SyntaxDepthExceeded,
    SyntaxError,
    UnresolvedInternalRequest,
    UnsupportedCommonjsExports,
    UnsupportedProjectTarget,
}

impl DiagnosticKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AmbiguousStarExport => "ambiguous_star_export",
            Self::EffectsBudgetExhausted => "effect_size_budget_exhausted",
            Self::FlowBudgetExhausted => "flow_link_budget_exhausted",
            Self::LinkingBudgetExhausted => "graph_link_budget_exhausted",
            Self::InvalidParserSpan => "invalid_parser_span",
            Self::MissingImportedExport => "missing_imported_export",
            Self::OutsideProjectTarget => "outside_project_target",
            Self::FactsBudgetExhausted => "semantic_budget_exhausted",
            Self::NameBudgetExhausted => "semantic_name_budget_exhausted",
            Self::ScopeShapeMismatch => "scope_shape_mismatch",
            Self::SourceTooLarge => "source_too_large",
            Self::SyntaxDepthExceeded => "syntax_depth_exceeded",
            Self::SyntaxError => "syntax_error",
            Self::UnresolvedInternalRequest => "unresolved_internal_request",
            Self::UnsupportedCommonjsExports => "unsupported_commonjs_exports",
            Self::UnsupportedProjectTarget => "unsupported_project_target",
        }
    }
}

impl DiagnosticCode {
    pub fn new(code: impl Into<String>) -> Result<Self, String> {
        let code = code.into();
        if !code.is_empty()
            && code.len() <= MAX_DIAGNOSTIC_CODE_LEN
            && code.chars().all(|character| {
                character.is_ascii_lowercase() || character == '_' || character.is_ascii_digit()
            })
            && code.as_bytes()[0].is_ascii_lowercase()
        {
            Ok(Self(code))
        } else {
            Err(code)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for DiagnosticCode {
    type Error = String;

    fn try_from(code: String) -> Result<Self, Self::Error> {
        Self::new(code)
    }
}

impl TryFrom<&str> for DiagnosticCode {
    type Error = String;

    fn try_from(code: &str) -> Result<Self, Self::Error> {
        Self::new(code)
    }
}

impl From<DiagnosticKind> for DiagnosticCode {
    fn from(kind: DiagnosticKind) -> Self {
        Self::new(kind.as_str()).expect("DiagnosticKind literals are canonical")
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SourceLocation {
    path: ProjectRelativePath,
    range: SourceRange,
}

impl SourceLocation {
    pub fn new(path: ProjectRelativePath, range: SourceRange) -> Self {
        Self { path, range }
    }

    pub fn path(&self) -> &ProjectRelativePath {
        &self.path
    }

    pub fn range(&self) -> SourceRange {
        self.range.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Evidence {
    message: String,
    #[cfg_attr(feature = "serde", serde(default))]
    count: u32,
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "std::ops::Not::not"))]
    truncated: bool,
    location: Option<SourceLocation>,
}

impl Evidence {
    pub fn new(
        message: String,
        count: u32,
        truncated: bool,
        location: Option<SourceLocation>,
    ) -> Self {
        Self {
            message,
            count,
            truncated,
            location,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    pub(crate) fn set_message(&mut self, message: String) {
        self.message = message;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Finding {
    rule_id: RuleId,
    message: String,
    severity: Severity,
    location: SourceLocation,
    evidence: EvidenceList,
}

impl Finding {
    pub fn new(
        rule_id: RuleId,
        message: String,
        severity: Severity,
        location: SourceLocation,
        evidence: EvidenceList,
    ) -> Self {
        Self {
            rule_id,
            message,
            severity,
            location,
            evidence,
        }
    }

    pub fn rule_id(&self) -> &RuleId {
        &self.rule_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn location(&self) -> &SourceLocation {
        &self.location
    }

    pub fn evidence(&self) -> &EvidenceList {
        &self.evidence
    }

    pub fn set_shared_evidence(&mut self, shared: std::sync::Arc<[Evidence]>) {
        self.evidence.set_shared(shared);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FileReport {
    path: ProjectRelativePath,
    findings: Vec<Finding>,
    diagnostics: Vec<Diagnostic>,
}

impl FileReport {
    pub fn new(
        path: ProjectRelativePath,
        findings: Vec<Finding>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            path,
            findings,
            diagnostics,
        }
    }

    pub fn path(&self) -> &ProjectRelativePath {
        &self.path
    }

    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub(crate) fn diagnostics_mut(&mut self) -> &mut Vec<Diagnostic> {
        &mut self.diagnostics
    }

    pub fn into_parts(self) -> (ProjectRelativePath, Vec<Finding>, Vec<Diagnostic>) {
        (self.path, self.findings, self.diagnostics)
    }

    #[must_use]
    pub fn has_parse_diagnostics(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d, Diagnostic::Parse { .. }))
    }

    #[must_use]
    pub fn parse_diagnostic_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| matches!(d, Diagnostic::Parse { .. }))
            .count()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum ReportCompletion {
    Complete,
    Partial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AnalysisDiagnostic {
    code: DiagnosticCode,
    message: String,
    location: Option<SourceLocation>,
}

impl AnalysisDiagnostic {
    pub fn new(code: DiagnosticCode, message: String, location: Option<SourceLocation>) -> Self {
        Self {
            code,
            message,
            location,
        }
    }

    pub fn code(&self) -> &DiagnosticCode {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    pub(crate) fn set_location(&mut self, location: Option<SourceLocation>) {
        self.location = location;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case", tag = "kind"))]
pub enum Diagnostic {
    Parse {
        path: ProjectRelativePath,
        diagnostic: crate::ParseDiagnostic,
    },
    Project(AnalysisDiagnostic),
}

impl Diagnostic {
    pub(crate) fn parse(path: ProjectRelativePath, diagnostic: crate::ParseDiagnostic) -> Self {
        Self::Parse { path, diagnostic }
    }

    pub(crate) fn project(diagnostic: AnalysisDiagnostic) -> Self {
        Self::Project(diagnostic)
    }

    fn inner(
        &self,
    ) -> (
        &str,
        &str,
        Option<&ProjectRelativePath>,
        Option<&SourceRange>,
    ) {
        match self {
            Self::Parse { path, diagnostic } => (
                diagnostic.code.as_str(),
                &diagnostic.message,
                Some(path),
                diagnostic.range.as_ref(),
            ),
            Self::Project(d) => (
                d.code.as_str(),
                &d.message,
                d.location.as_ref().map(|l| &l.path),
                d.location.as_ref().map(|l| &l.range),
            ),
        }
    }

    #[must_use]
    pub fn code(&self) -> &str {
        self.inner().0
    }

    #[must_use]
    pub fn message(&self) -> &str {
        self.inner().1
    }

    #[must_use]
    pub fn path(&self) -> Option<&ProjectRelativePath> {
        self.inner().2
    }

    #[must_use]
    pub fn range(&self) -> Option<&SourceRange> {
        self.inner().3
    }

    #[must_use]
    pub fn parse_diagnostic(&self) -> Option<&crate::ParseDiagnostic> {
        match self {
            Self::Parse { diagnostic, .. } => Some(diagnostic),
            Self::Project(_) => None,
        }
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
            findings: self.files.iter().map(|f| f.findings.len()).sum(),
            parse_diagnostics: self
                .files
                .iter()
                .flat_map(|f| f.diagnostics.iter())
                .filter(|d| matches!(d, Diagnostic::Parse { .. }))
                .count(),
            file_diagnostics: self
                .files
                .iter()
                .flat_map(|f| f.diagnostics.iter())
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AnalysisOperationCounts {
    files: usize,
    requests: usize,
    edges: usize,
    exports: usize,
    scc_rounds: usize,
    effect_projections: usize,
    evidence: usize,
}

impl AnalysisOperationCounts {
    pub fn new(
        files: usize,
        requests: usize,
        edges: usize,
        exports: usize,
        scc_rounds: usize,
        effect_projections: usize,
        evidence: usize,
    ) -> Self {
        Self {
            files,
            requests,
            edges,
            exports,
            scc_rounds,
            effect_projections,
            evidence,
        }
    }

    pub fn files(&self) -> usize {
        self.files
    }

    pub fn requests(&self) -> usize {
        self.requests
    }

    pub fn edges(&self) -> usize {
        self.edges
    }

    pub fn exports(&self) -> usize {
        self.exports
    }

    pub fn scc_rounds(&self) -> usize {
        self.scc_rounds
    }

    pub fn effect_projections(&self) -> usize {
        self.effect_projections
    }

    pub fn evidence(&self) -> usize {
        self.evidence
    }

    pub(crate) fn set_effect_projections(&mut self, value: usize) {
        self.effect_projections = value;
    }

    pub fn into_parts(self) -> (usize, usize, usize, usize, usize, usize, usize) {
        (
            self.files,
            self.requests,
            self.edges,
            self.exports,
            self.scc_rounds,
            self.effect_projections,
            self.evidence,
        )
    }
}

impl std::ops::AddAssign for AnalysisOperationCounts {
    fn add_assign(&mut self, rhs: Self) {
        self.files = self.files.saturating_add(rhs.files);
        self.requests = self.requests.saturating_add(rhs.requests);
        self.edges = self.edges.saturating_add(rhs.edges);
        self.exports = self.exports.saturating_add(rhs.exports);
        self.scc_rounds = self.scc_rounds.saturating_add(rhs.scc_rounds);
        self.effect_projections = self
            .effect_projections
            .saturating_add(rhs.effect_projections);
        self.evidence = self.evidence.saturating_add(rhs.evidence);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_kind_table_contains_only_canonical_codes() {
        let kinds = [
            DiagnosticKind::AmbiguousStarExport,
            DiagnosticKind::EffectsBudgetExhausted,
            DiagnosticKind::FlowBudgetExhausted,
            DiagnosticKind::LinkingBudgetExhausted,
            DiagnosticKind::InvalidParserSpan,
            DiagnosticKind::MissingImportedExport,
            DiagnosticKind::OutsideProjectTarget,
            DiagnosticKind::FactsBudgetExhausted,
            DiagnosticKind::NameBudgetExhausted,
            DiagnosticKind::ScopeShapeMismatch,
            DiagnosticKind::SourceTooLarge,
            DiagnosticKind::SyntaxDepthExceeded,
            DiagnosticKind::SyntaxError,
            DiagnosticKind::UnresolvedInternalRequest,
            DiagnosticKind::UnsupportedCommonjsExports,
            DiagnosticKind::UnsupportedProjectTarget,
        ];
        for kind in kinds {
            let owned: DiagnosticCode = kind.into();
            assert_eq!(DiagnosticCode::try_from(kind.as_str()), Ok(owned));
        }
    }
}
