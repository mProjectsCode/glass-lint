use crate::{EvidenceList, RuleId, Severity, SourceRange, project::types::ProjectRelativePath};

/// Stable machine-readable identity for a project diagnostic.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
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

impl<'de> serde::Deserialize<'de> for DiagnosticCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
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

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SourceLocation {
    pub path: ProjectRelativePath,
    pub range: SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Evidence {
    pub message: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub evidence_truncated: bool,
    pub location: Option<SourceLocation>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Finding {
    pub rule_id: RuleId,
    pub message_id: String,
    pub message: String,
    pub severity: Severity,
    pub location: SourceLocation,
    pub evidence: EvidenceList,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FileReport {
    pub path: ProjectRelativePath,
    pub findings: Vec<Finding>,
    pub diagnostics: Vec<Diagnostic>,
}

impl FileReport {
    #[must_use]
    pub fn has_parse_diagnostics(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic, Diagnostic::Parse { .. }))
    }

    #[must_use]
    pub fn parse_diagnostic_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| matches!(diagnostic, Diagnostic::Parse { .. }))
            .count()
    }
}

/// Whether the project was analyzed to completion.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum ReportCompletion {
    Complete,
    Partial,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AnalysisDiagnostic {
    pub code: DiagnosticCode,
    pub message: String,
    pub location: Option<SourceLocation>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
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
            Self::Project(diagnostic) => (
                diagnostic.code.as_str(),
                &diagnostic.message,
                diagnostic.location.as_ref().map(|loc| &loc.path),
                diagnostic.location.as_ref().map(|loc| &loc.range),
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

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AnalysisReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub files: Vec<FileReport>,
    pub diagnostics: Vec<Diagnostic>,
    pub operations: AnalysisOperationCounts,
    pub completion: ReportCompletion,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnalysisReportSummary {
    pub files: usize,
    pub findings: usize,
    pub parse_diagnostics: usize,
    pub file_diagnostics: usize,
    pub report_diagnostics: usize,
}

impl AnalysisReport {
    pub fn summary(&self) -> AnalysisReportSummary {
        AnalysisReportSummary {
            files: self.files.len(),
            findings: self.files.iter().map(|file| file.findings.len()).sum(),
            parse_diagnostics: self
                .files
                .iter()
                .flat_map(|file| file.diagnostics.iter())
                .filter(|diagnostic| matches!(diagnostic, Diagnostic::Parse { .. }))
                .count(),
            file_diagnostics: self
                .files
                .iter()
                .flat_map(|file| file.diagnostics.iter())
                .filter(|diagnostic| matches!(diagnostic, Diagnostic::Project(_)))
                .count(),
            report_diagnostics: self
                .diagnostics
                .iter()
                .filter(|diagnostic| matches!(diagnostic, Diagnostic::Project(_)))
                .count(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AnalysisOperationCounts {
    pub files: usize,
    pub requests: usize,
    pub edges: usize,
    pub exports: usize,
    pub scc_rounds: usize,
    pub effect_projections: usize,
    pub evidence: usize,
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
