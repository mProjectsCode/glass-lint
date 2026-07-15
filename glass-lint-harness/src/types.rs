use std::collections::BTreeMap;

use glass_lint_core::{Finding, Severity};
use serde::{Deserialize, Serialize};

pub const ADAPTER_PROTOCOL_VERSION: u32 = 3;

#[derive(Clone, Debug)]
pub struct Case {
    pub id: String,
    pub description: String,
    pub tags: Vec<String>,
    pub language: String,
    pub filename: String,
    pub source: String,
    pub project: Option<ProjectCase>,
    pub tools: BTreeMap<String, ToolExpectation>,
}

/// A multi-file harness input. Paths are project-relative and sources are
/// retained in sorted order so virtual and filesystem cases have identical
/// identities.
#[derive(Clone, Debug)]
pub struct ProjectCase {
    pub root: std::path::PathBuf,
    pub entries: Vec<String>,
    pub files: Vec<ProjectFile>,
    pub resolutions: Vec<ProjectResolution>,
    pub filesystem: bool,
}

#[derive(Clone, Debug)]
pub struct ProjectFile {
    pub path: String,
    pub language: String,
    pub source: String,
}

#[derive(Clone, Debug)]
pub struct ProjectResolution {
    pub importer: String,
    pub kind: String,
    pub request: String,
    pub range: glass_lint_core::SourceRange,
    pub result: ProjectResolutionResult,
}

#[derive(Clone, Debug)]
pub enum ProjectResolutionResult {
    Internal { path: String },
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
    Unsupported { reason: String },
}

impl From<ProjectFile> for AdapterFile {
    fn from(file: ProjectFile) -> Self {
        Self {
            path: file.path,
            language: file.language,
            source: file.source,
        }
    }
}
impl From<AdapterFile> for ProjectFile {
    fn from(file: AdapterFile) -> Self {
        Self {
            path: file.path,
            language: file.language,
            source: file.source,
        }
    }
}

impl From<ProjectResolutionResult> for AdapterResolutionResult {
    fn from(result: ProjectResolutionResult) -> Self {
        match result {
            ProjectResolutionResult::Internal { path } => Self::Internal { path },
            ProjectResolutionResult::External { package } => Self::External { package },
            ProjectResolutionResult::Builtin { name } => Self::Builtin { name },
            ProjectResolutionResult::Missing => Self::Missing,
            ProjectResolutionResult::OutsideProject { path } => Self::OutsideProject { path },
            ProjectResolutionResult::Unsupported { reason } => Self::Unsupported { reason },
        }
    }
}
impl From<AdapterResolutionResult> for ProjectResolutionResult {
    fn from(result: AdapterResolutionResult) -> Self {
        match result {
            AdapterResolutionResult::Internal { path } => Self::Internal { path },
            AdapterResolutionResult::External { package } => Self::External { package },
            AdapterResolutionResult::Builtin { name } => Self::Builtin { name },
            AdapterResolutionResult::Missing => Self::Missing,
            AdapterResolutionResult::OutsideProject { path } => Self::OutsideProject { path },
            AdapterResolutionResult::Unsupported { reason } => Self::Unsupported { reason },
        }
    }
}

impl From<ProjectResolution> for AdapterResolution {
    fn from(resolution: ProjectResolution) -> Self {
        Self {
            importer: resolution.importer,
            kind: resolution.kind,
            request: resolution.request,
            range: resolution.range,
            result: resolution.result.into(),
        }
    }
}
impl From<AdapterResolution> for ProjectResolution {
    fn from(resolution: AdapterResolution) -> Self {
        Self {
            importer: resolution.importer,
            kind: resolution.kind,
            request: resolution.request,
            range: resolution.range,
            result: resolution.result.into(),
        }
    }
}

impl From<ProjectCase> for AdapterProject {
    fn from(project: ProjectCase) -> Self {
        Self {
            root: project.root.to_string_lossy().into_owned(),
            entries: project.entries,
            files: project.files.into_iter().map(Into::into).collect(),
            resolutions: project.resolutions.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<AdapterProject> for ProjectCase {
    fn from(project: AdapterProject) -> Self {
        Self {
            root: project.root.into(),
            entries: project.entries,
            files: project.files.into_iter().map(Into::into).collect(),
            resolutions: project.resolutions.into_iter().map(Into::into).collect(),
            filesystem: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolExpectation {
    pub config: Option<String>,
    pub rules: Vec<String>,
    pub required: Vec<DiagnosticExpectation>,
    pub forbidden: Vec<DiagnosticExpectation>,
}

#[derive(Clone, Debug)]
pub struct DiagnosticExpectation {
    pub path: Option<String>,
    pub rule_id: String,
    pub message_id: Option<String>,
    pub severity: Option<Severity>,
    pub count: Option<usize>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdapterRequest {
    pub protocol_version: u32,
    pub case_id: String,
    pub filename: String,
    pub language: String,
    pub source: String,
    pub rules: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<AdapterProject>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdapterProject {
    pub root: String,
    pub entries: Vec<String>,
    pub files: Vec<AdapterFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolutions: Vec<AdapterResolution>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdapterFile {
    pub path: String,
    pub language: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdapterResolution {
    pub importer: String,
    pub kind: String,
    pub request: String,
    pub range: glass_lint_core::SourceRange,
    pub result: AdapterResolutionResult,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdapterResolutionResult {
    Internal { path: String },
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
    Unsupported { reason: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdapterResponse {
    pub protocol_version: u32,
    pub tool: String,
    pub tool_version: String,
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub finding_locations: Vec<FindingLocation>,
}

#[derive(Clone, Debug)]
pub struct AdapterRun {
    pub findings: Vec<Finding>,
    pub finding_locations: Vec<FindingLocation>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CaseResult {
    pub id: String,
    pub description: String,
    pub source: String,
    pub tools: BTreeMap<String, ToolResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolResult {
    pub version: String,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub passed: bool,
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finding_locations: Vec<FindingLocation>,
    pub errors: Vec<String>,
}

/// File-qualified locations retained by the harness report. The core
/// `Finding` remains the stable single-file compatibility shape.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FindingLocation {
    pub primary: Option<String>,
    pub evidence: Vec<Option<String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SuiteReport {
    pub schema_version: u32,
    pub cases: Vec<CaseResult>,
}

impl SuiteReport {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.cases
            .iter()
            .all(|case| case.tools.values().all(|tool| tool.passed))
    }
}
