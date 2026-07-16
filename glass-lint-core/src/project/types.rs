//! Public project input, resolution, and report contracts.
//!
//! These types make project analysis filesystem-free: callers provide authored
//! sources and explicit resolver outcomes, and reports retain normalized paths
//! and source ranges for deterministic downstream rendering.

use std::{
    borrow::Borrow,
    ops::Deref,
    path::{Path, PathBuf},
};

use crate::{SourceLanguage, SourceRange};

/// Whether a module request uses syntax that denotes an authored/internal
/// target.
pub fn is_internal_module_request(request: &str) -> bool {
    request.starts_with('.') || request.starts_with('/') || request.starts_with('#')
}

/// A normalized project-relative path whose representation cannot be mutated
/// back into an absolute or escaping path by callers.
#[derive(
    Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, serde::Deserialize, serde::Serialize,
)]
#[serde(transparent)]
pub struct ProjectRelativePath(String);

impl ProjectRelativePath {
    pub(crate) fn from_normalized(path: String) -> Self {
        Self(path)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for ProjectRelativePath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Deref for ProjectRelativePath {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ProjectRelativePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for ProjectRelativePath {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl std::fmt::Display for ProjectRelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for ProjectRelativePath {
    fn from(path: String) -> Self {
        Self(path)
    }
}

impl From<&str> for ProjectRelativePath {
    fn from(path: &str) -> Self {
        Self(path.into())
    }
}

impl PartialEq<&str> for ProjectRelativePath {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// Stable machine-readable identity for a project diagnostic.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct DiagnosticCode(String);

impl DiagnosticCode {
    /// Validate and construct a diagnostic code in the documented identifier
    /// form.
    pub fn new(code: impl Into<String>) -> Result<Self, String> {
        let code = code.into();
        if !code.is_empty()
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

impl From<&str> for DiagnosticCode {
    fn from(code: &str) -> Self {
        Self::new(code).expect("diagnostic code literals must be valid identifiers")
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct SourceFile {
    /// Normalized project-relative source path.
    pub path: ProjectRelativePath,
    /// Parser language selected for this source.
    pub language: SourceLanguage,
    /// Source text to parse and analyze.
    pub source: String,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum ResolutionRequestKind {
    /// A static ES module import.
    Import,
    /// A dynamic `import()` request.
    DynamicImport,
    /// A CommonJS `require()` request.
    Require,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct ResolutionRequestKey {
    /// Normalized path of the file containing the request.
    pub importer: ProjectRelativePath,
    /// Syntax family that produced the request.
    pub kind: ResolutionRequestKind,
    /// Exact source range identifying the request.
    pub range: SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ResolutionRequest {
    /// Stable identity of the request in its importer.
    pub key: ResolutionRequestKey,
    /// Literal request string supplied to the resolver.
    pub request: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ResolutionResult {
    /// Resolve to another authored project source.
    Internal { path: ProjectRelativePath },
    /// Resolve to a package outside the authored project.
    External { package: String },
    /// Resolve to a runtime-provided builtin module.
    Builtin { name: String },
    /// State that no target was found.
    Missing,
    /// Resolve to a path deliberately outside the project root.
    OutsideProject { path: String },
    /// Preserve a resolver state the linker cannot interpret.
    Unsupported { reason: String },
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct ModuleId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ResolvedModule {
    /// An authored module, identified by its stable project ID and path.
    Internal {
        id: ModuleId,
        path: ProjectRelativePath,
    },
    /// A package boundary rather than an authored module.
    External { package: String },
    /// A runtime-provided builtin module.
    Builtin { name: String },
    /// No target was available.
    Missing,
    /// A target outside the analyzed project.
    OutsideProject { path: String },
    /// A resolver outcome that cannot be linked precisely.
    Unsupported { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SourceLocation {
    /// Normalized path containing the location.
    pub path: ProjectRelativePath,
    /// Exact source range within that path.
    pub range: SourceRange,
}
/// Evidence attached to a project finding.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectEvidence {
    /// Human-readable explanation of the evidence.
    pub message: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub evidence_truncated: bool,
    /// Optional source location supporting the explanation.
    pub location: Option<SourceLocation>,
    /// Optional originating rule or evidence source identifier.
    pub source: Option<String>,
}
/// A rule finding whose location and evidence are project-qualified.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectFinding {
    /// Fully qualified rule identifier.
    pub rule_id: crate::RuleId,
    /// Stable message identifier within the rule.
    pub message_id: String,
    /// Rendered finding message.
    pub message: String,
    /// Finding severity.
    pub severity: crate::Severity,
    /// Primary project-qualified source location.
    pub location: SourceLocation,
    /// Supporting evidence in deterministic de-duplicated order.
    pub evidence: super::EvidenceList,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectFileReport {
    /// Normalized path of the analyzed file.
    pub path: ProjectRelativePath,
    /// Findings attributed to this file.
    pub findings: Vec<ProjectFinding>,
    /// Parser diagnostics attributed to this file.
    pub parse_diagnostics: Vec<crate::ParseDiagnostic>,
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
/// A project-level diagnostic not owned by one finding.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectDiagnostic {
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Optional project source location.
    pub location: Option<SourceLocation>,
}
/// Complete deterministic output of one project analysis.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectReport {
    /// Version of the serialized project-report contract.
    pub schema_version: u32,
    /// Tool version that produced the report.
    pub tool_version: String,
    /// Per-file findings and parse diagnostics in normalized path order.
    pub files: Vec<ProjectFileReport>,
    /// Diagnostics that are not owned by a single file.
    pub diagnostics: Vec<ProjectDiagnostic>,
    /// Bounded operation counters collected during analysis.
    pub operations: ProjectOperationCounts,
    /// Partial reports are diagnostic output and must fail the CLI run.
    pub completion: ReportCompletion,
}

/// Counts used by front ends when rendering a project report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectReportSummary {
    /// Number of reported source files.
    pub files: usize,
    /// Number of findings across all files.
    pub findings: usize,
    /// Number of file-owned parse diagnostics.
    pub parse_diagnostics: usize,
    /// Number of project-level diagnostics.
    pub project_diagnostics: usize,
}

impl ProjectReport {
    /// Summarize the report from its canonical file and diagnostic collections.
    pub fn summary(&self) -> ProjectReportSummary {
        ProjectReportSummary {
            files: self.files.len(),
            findings: self.files.iter().map(|file| file.findings.len()).sum(),
            parse_diagnostics: self
                .files
                .iter()
                .map(|file| file.parse_diagnostics.len())
                .sum(),
            project_diagnostics: self.diagnostics.len(),
        }
    }
}
/// Bounded counters describing work performed while linking the project.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectOperationCounts {
    /// Number of sources analyzed.
    pub files: usize,
    /// Number of authored resolution requests.
    pub requests: usize,
    /// Number of linked module edges.
    pub edges: usize,
    /// Number of exported bindings examined.
    pub exports: usize,
    /// Number of strongly connected component propagation rounds.
    pub scc_rounds: usize,
    /// Number of effect summaries projected across module edges.
    pub effect_projections: usize,
    /// Number of evidence records emitted.
    pub evidence: usize,
}

impl std::ops::AddAssign for ProjectOperationCounts {
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
/// Unvalidated caller-supplied project sources and resolver answers.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectInput {
    /// Root used to validate the project-relative path namespace.
    pub root: PathBuf,
    /// Authored source files to parse and analyze.
    pub sources: Vec<SourceFile>,
    /// Explicit resolver answers keyed by request location and kind.
    pub resolutions: Vec<(ResolutionRequestKey, ResolutionResult)>,
}

/// Validation failures for project inputs and explicit resolver answers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectInputError {
    /// A path is empty, absolute, escapes, or otherwise malformed.
    InvalidPath(String),
    /// Two authored sources normalize to the same path.
    DuplicateSource(String),
    /// A resolution refers to a non-authored importer.
    UnknownImporter(String),
    /// A resolution range is malformed or outside its source.
    InvalidRange(String),
    /// More than one answer was supplied for a request key.
    DuplicateResolution(ResolutionRequestKey),
    /// A resolution target violates the target-path contract.
    InvalidTarget(String),
    /// No authored request matches the supplied resolution key.
    UnknownRequest(ResolutionRequestKey),
    /// A configured project budget was exceeded.
    BudgetExceeded(String),
}
impl std::fmt::Display for ProjectInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(path) => write!(f, "invalid project path `{path}`"),
            Self::DuplicateSource(path) => write!(f, "duplicate project source `{path}`"),
            Self::UnknownImporter(path) => {
                write!(f, "resolution importer is not a source: `{path}`")
            }
            Self::InvalidRange(path) => write!(f, "resolution range is invalid for `{path}`"),
            Self::DuplicateResolution(key) => {
                write!(f, "duplicate resolution for `{}`", key.importer)
            }
            Self::InvalidTarget(path) => write!(f, "invalid resolution target `{path}`"),
            Self::UnknownRequest(key) => write!(
                f,
                "resolution does not match an authored request in `{}`",
                key.importer
            ),
            Self::BudgetExceeded(message) => write!(f, "project input budget exceeded: {message}"),
        }
    }
}
impl std::error::Error for ProjectInputError {}

impl SourceFile {
    /// Construct a source file and infer its parser language from its path.
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            language: SourceLanguage::from_filename(&path),
            path: path.into(),
            source: source.into(),
        }
    }
}
