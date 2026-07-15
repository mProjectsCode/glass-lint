//! Public project input, resolution, and report contracts.

use std::path::PathBuf;

use crate::{SourceLanguage, SourceRange};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct SourceFile {
    pub path: String,
    pub language: SourceLanguage,
    pub source: String,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum ResolutionRequestKind {
    Import,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct ResolutionRequestKey {
    pub importer: String,
    pub kind: ResolutionRequestKind,
    pub range: SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ResolutionRequest {
    pub key: ResolutionRequestKey,
    pub request: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ResolutionResult {
    Internal { path: String },
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
    Unsupported { reason: String },
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub(crate) struct ModuleId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) enum ResolvedModule {
    Internal { id: ModuleId, path: String },
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
    Unsupported { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SourceLocation {
    pub path: String,
    pub range: SourceRange,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectEvidence {
    pub message: String,
    pub location: Option<SourceLocation>,
    pub source: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectFinding {
    pub rule_id: crate::RuleId,
    pub message_id: String,
    pub message: String,
    pub severity: crate::Severity,
    pub location: SourceLocation,
    pub evidence: super::EvidenceList,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectFileReport {
    pub path: String,
    pub findings: Vec<ProjectFinding>,
    pub parse_diagnostics: Vec<crate::ParseDiagnostic>,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectDiagnostic {
    pub code: String,
    pub message: String,
    pub location: Option<SourceLocation>,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub files: Vec<ProjectFileReport>,
    pub diagnostics: Vec<ProjectDiagnostic>,
    pub operations: ProjectOperationCounts,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectOperationCounts {
    pub files: usize,
    pub requests: usize,
    pub edges: usize,
    pub exports: usize,
    pub scc_rounds: usize,
    pub effect_projections: usize,
    pub evidence: usize,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectInput {
    pub root: PathBuf,
    pub sources: Vec<SourceFile>,
    pub resolutions: Vec<(ResolutionRequestKey, ResolutionResult)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectInputError {
    InvalidPath(String),
    DuplicateSource(String),
    UnknownImporter(String),
    InvalidRange(String),
    DuplicateResolution(ResolutionRequestKey),
    InvalidTarget(String),
    UnknownRequest(ResolutionRequestKey),
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
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            language: SourceLanguage::from_filename(&path),
            path,
            source: source.into(),
        }
    }
}
