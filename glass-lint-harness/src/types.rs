use std::collections::BTreeMap;

use glass_lint_core::{Finding, ResolutionRequestKind, ResolutionResult, Severity};
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
    pub files: Vec<AdapterFile>,
    pub resolutions: Vec<AdapterResolution>,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdapterProject {
    pub root: String,
    pub entries: Vec<String>,
    pub files: Vec<AdapterFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolutions: Vec<AdapterResolution>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdapterFile {
    pub path: String,
    pub language: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdapterResolution {
    pub importer: String,
    pub kind: AdapterResolutionKind,
    pub request: String,
    pub range: glass_lint_core::SourceRange,
    pub result: AdapterResolutionResult,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterResolutionKind {
    Import,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdapterResolutionResult {
    Internal { path: String },
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
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
            AdapterResolutionResult::Internal { path } => {
                ResolutionResult::Internal { path: path.clone() }
            }
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
