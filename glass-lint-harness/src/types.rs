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

#[derive(Clone, Debug)]
pub struct ToolExpectation {
    pub config: Option<String>,
    pub rules: Vec<String>,
    pub required: Vec<DiagnosticExpectation>,
    pub forbidden: Vec<DiagnosticExpectation>,
}

#[derive(Clone, Debug)]
pub struct DiagnosticExpectation {
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
#[derive(Clone, Debug, Serialize)]
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
