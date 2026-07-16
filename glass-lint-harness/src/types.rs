//! Case, adapter-protocol, result, and profiling data contracts.

use std::collections::BTreeMap;

use glass_lint_core::{Finding, ResolutionRequestKind, ResolutionResult, Severity};
use serde::{Deserialize, Serialize};

pub const ADAPTER_PROTOCOL_VERSION: u32 = 3;

#[derive(Clone, Debug)]
/// One source fixture and its per-tool expectations.
pub struct Case {
    /// Stable path-derived case identifier.
    pub id: String,
    /// Human-readable case description.
    pub description: String,
    /// Tags used by fixture consumers.
    pub tags: Vec<String>,
    /// Adapter protocol language name.
    pub language: String,
    /// Filename used for parser and location semantics.
    pub filename: String,
    /// Source text, including expectation directives.
    pub source: String,
    /// Optional multi-file project input.
    pub project: Option<ProjectCase>,
    /// Expectations keyed by adapter name.
    pub tools: BTreeMap<String, ToolExpectation>,
}

/// A multi-file harness input. Paths are project-relative and sources are
/// retained in sorted order so virtual and filesystem cases have identical
/// identities.
#[derive(Clone, Debug)]
pub struct ProjectCase {
    /// Root namespace for project-relative paths.
    pub root: std::path::PathBuf,
    /// Entry paths selected for project analysis.
    pub entries: Vec<String>,
    /// Authored project files in normalized order.
    pub files: Vec<AdapterFile>,
    /// Explicit resolver answers for project requests.
    pub resolutions: Vec<AdapterResolution>,
    /// Whether the adapter should load the real filesystem tree.
    pub filesystem: bool,
}

impl From<&ProjectCase> for AdapterProject {
    fn from(project: &ProjectCase) -> Self {
        Self {
            root: project.root.to_string_lossy().into_owned(),
            entries: project.entries.clone(),
            files: project.files.clone(),
            resolutions: project.resolutions.clone(),
        }
    }
}
impl From<AdapterProject> for ProjectCase {
    fn from(project: AdapterProject) -> Self {
        Self {
            root: project.root.into(),
            entries: project.entries,
            files: project.files,
            resolutions: project.resolutions,
            filesystem: false,
        }
    }
}

#[derive(Clone, Debug)]
/// Expectations for one adapter on one case.
pub struct ToolExpectation {
    /// Named adapter configuration, if any.
    pub config: Option<String>,
    /// Explicit rule IDs to enable.
    pub rules: Vec<String>,
    /// Findings that must be present.
    pub required: Vec<DiagnosticExpectation>,
    /// Findings that must be absent.
    pub forbidden: Vec<DiagnosticExpectation>,
}

#[derive(Clone, Debug)]
pub struct DiagnosticExpectation {
    /// Optional project-relative finding path.
    pub path: Option<String>,
    /// Stable rule ID to compare.
    pub rule_id: String,
    /// Optional message ID constraint.
    pub message_id: Option<String>,
    /// Optional severity constraint.
    pub severity: Option<Severity>,
    /// Exact expected count when specified.
    pub count: Option<usize>,
    /// Optional one-based source line.
    pub line: Option<u32>,
    /// Optional one-based source column.
    pub column: Option<u32>,
    /// Optional rendered-message constraint.
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdapterRequest {
    /// Protocol version negotiated by the harness.
    pub protocol_version: u32,
    /// Case identity for adapter diagnostics.
    pub case_id: String,
    /// Source filename and language metadata.
    pub filename: String,
    pub language: String,
    pub source: String,
    pub rules: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<AdapterProject>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdapterProject {
    /// Project root sent to the adapter.
    pub root: String,
    /// Project entry paths.
    pub entries: Vec<String>,
    /// Authored project files.
    pub files: Vec<AdapterFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolutions: Vec<AdapterResolution>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdapterFile {
    /// Project-relative file path.
    pub path: String,
    /// Adapter language identifier.
    pub language: String,
    /// File source text.
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdapterResolution {
    /// Importer file path.
    pub importer: String,
    /// Request syntax kind.
    pub kind: AdapterResolutionKind,
    /// Literal resolver request.
    pub request: String,
    /// Exact request source range.
    pub range: glass_lint_core::SourceRange,
    /// Typed resolver outcome.
    pub result: AdapterResolutionResult,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterResolutionKind {
    /// Static import request.
    Import,
    /// Dynamic import request.
    DynamicImport,
    /// CommonJS require request.
    Require,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdapterResolutionResult {
    /// Authored internal target.
    Internal { path: String },
    /// External package target.
    External { package: String },
    /// Runtime builtin target.
    Builtin { name: String },
    /// Unresolved request.
    Missing,
    /// Deliberately outside-project target.
    OutsideProject { path: String },
    /// Unsupported resolver state.
    Unsupported { reason: String },
}

/// Converts the protocol's validated resolution representation at the core
/// project-session boundary. Keeping this conversion here prevents adapters
/// and manifest parsing from maintaining parallel core-facing DTOs.
impl TryFrom<&AdapterResolution> for (ResolutionRequestKind, ResolutionResult) {
    type Error = String;

    fn try_from(resolution: &AdapterResolution) -> Result<Self, Self::Error> {
        let kind = match resolution.kind {
            AdapterResolutionKind::Import => ResolutionRequestKind::Import,
            AdapterResolutionKind::DynamicImport => ResolutionRequestKind::DynamicImport,
            AdapterResolutionKind::Require => ResolutionRequestKind::Require,
        };
        let result = match &resolution.result {
            AdapterResolutionResult::Internal { path } => ResolutionResult::Internal {
                path: path.clone().into(),
            },
            AdapterResolutionResult::External { package } => ResolutionResult::External {
                package: package.clone(),
            },
            AdapterResolutionResult::Builtin { name } => {
                ResolutionResult::Builtin { name: name.clone() }
            }
            AdapterResolutionResult::Missing => ResolutionResult::Missing,
            AdapterResolutionResult::OutsideProject { path } => {
                ResolutionResult::OutsideProject { path: path.clone() }
            }
            AdapterResolutionResult::Unsupported { reason } => ResolutionResult::Unsupported {
                reason: reason.clone(),
            },
        };
        Ok((kind, result))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdapterResponse {
    /// Protocol version echoed by the adapter.
    pub protocol_version: u32,
    /// Adapter identity validated by the runner.
    pub tool: String,
    /// Adapter-reported tool version.
    pub tool_version: String,
    /// Normalized findings.
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub finding_locations: Vec<FindingLocation>,
}

#[derive(Clone, Debug)]
pub struct AdapterRun {
    /// Findings produced by one adapter invocation.
    pub findings: Vec<Finding>,
    /// Optional file ownership aligned by finding index.
    pub finding_locations: Vec<FindingLocation>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CaseResult {
    /// Stable case identifier.
    pub id: String,
    /// Case description.
    pub description: String,
    /// Original case source for report context.
    pub source: String,
    /// Results keyed by adapter name.
    pub tools: BTreeMap<String, ToolResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolResult {
    /// Adapter version used for this run.
    pub version: String,
    /// Whether execution was intentionally skipped.
    pub skipped: bool,
    /// Explanation for a skipped execution.
    pub skip_reason: Option<String>,
    /// Whether all expectations passed.
    pub passed: bool,
    /// Findings returned by the adapter.
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finding_locations: Vec<FindingLocation>,
    pub errors: Vec<String>,
}

/// File-qualified locations retained by the harness report. The core
/// `Finding` remains the stable single-file compatibility shape.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FindingLocation {
    /// Primary file owning the finding.
    pub primary: Option<String>,
    /// Evidence file paths aligned with the finding evidence list.
    pub evidence: Vec<Option<String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SuiteReport {
    /// Serialized report schema version.
    pub schema_version: u32,
    /// Case results in deterministic discovery order.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn resolution(
        kind: AdapterResolutionKind,
        result: AdapterResolutionResult,
    ) -> AdapterResolution {
        AdapterResolution {
            importer: "main.js".into(),
            kind,
            request: "request".into(),
            range: glass_lint_core::SourceRange {
                start: glass_lint_core::Position { line: 1, column: 2 },
                end: glass_lint_core::Position { line: 1, column: 8 },
            },
            result,
        }
    }

    #[test]
    fn adapter_project_protocol_json_preserves_all_resolution_variants() {
        let project = AdapterProject {
            root: "/tmp/project".into(),
            entries: vec!["main.js".into()],
            files: vec![AdapterFile {
                path: "main.js".into(),
                language: "javascript".into(),
                source: "import x from 'x';".into(),
            }],
            resolutions: vec![
                resolution(
                    AdapterResolutionKind::Import,
                    AdapterResolutionResult::Internal {
                        path: "lib.js".into(),
                    },
                ),
                resolution(
                    AdapterResolutionKind::DynamicImport,
                    AdapterResolutionResult::External {
                        package: "pkg".into(),
                    },
                ),
                resolution(
                    AdapterResolutionKind::Require,
                    AdapterResolutionResult::Builtin { name: "fs".into() },
                ),
                resolution(
                    AdapterResolutionKind::Import,
                    AdapterResolutionResult::Missing,
                ),
                resolution(
                    AdapterResolutionKind::DynamicImport,
                    AdapterResolutionResult::OutsideProject {
                        path: "../outside.js".into(),
                    },
                ),
                resolution(
                    AdapterResolutionKind::Require,
                    AdapterResolutionResult::Unsupported {
                        reason: "dynamic target".into(),
                    },
                ),
            ],
        };
        let json = serde_json::to_value(&project).unwrap();
        assert_eq!(json["resolutions"][0]["kind"], "import");
        assert_eq!(json["resolutions"][1]["kind"], "dynamic_import");
        assert_eq!(json["resolutions"][2]["kind"], "require");
        assert_eq!(json["resolutions"][0]["result"]["kind"], "internal");
        assert_eq!(json["resolutions"][1]["result"]["kind"], "external");
        assert_eq!(json["resolutions"][2]["result"]["kind"], "builtin");
        assert_eq!(json["resolutions"][3]["result"]["kind"], "missing");
        assert_eq!(json["resolutions"][4]["result"]["kind"], "outside_project");
        assert_eq!(json["resolutions"][5]["result"]["kind"], "unsupported");
    }

    #[test]
    fn adapter_project_round_trips_protocol_data() {
        let project = AdapterProject {
            root: "/tmp/project".into(),
            entries: vec!["main.js".into()],
            files: vec![AdapterFile {
                path: "main.js".into(),
                language: "javascript".into(),
                source: "fetch('/');".into(),
            }],
            resolutions: Vec::new(),
        };
        let encoded = serde_json::to_string(&project).unwrap();
        let decoded: AdapterProject = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, project);
    }
}
