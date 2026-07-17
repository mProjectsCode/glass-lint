//! Case, adapter-protocol, result, and profiling data contracts.

use std::collections::BTreeMap;

use glass_lint_core::{
    Finding, ProjectRelativePath, ResolutionRequestKind, ResolutionResult, Severity,
};
use serde::{Deserialize, Serialize};

pub const ADAPTER_PROTOCOL_VERSION: u32 = 3;

#[derive(Clone, Debug)]
/// One source fixture and its per-tool expectations.
pub struct Case {
    /// Stable path-derived case identifier.
    pub(crate) id: String,
    /// Human-readable case description.
    pub(crate) description: String,
    /// Tags used by fixture consumers.
    pub(crate) tags: Vec<String>,
    /// Adapter protocol language name.
    pub(crate) language: String,
    /// Filename used for parser and location semantics.
    pub(crate) filename: String,
    /// Source text, including expectation directives.
    pub(crate) source: String,
    /// Optional multi-file project input.
    pub(crate) project: Option<ProjectCase>,
    /// Expectations keyed by adapter name.
    pub(crate) tools: BTreeMap<String, ToolExpectation>,
}

impl Case {
    /// Construct a fixture case with the required identity fields validated.
    pub fn new(
        id: impl Into<String>,
        description: impl Into<String>,
        language: impl Into<String>,
        filename: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, String> {
        let id = id.into();
        let language = language.into();
        let filename = filename.into();
        if id.trim().is_empty() || language.trim().is_empty() || filename.trim().is_empty() {
            return Err("case id, language, and filename must not be empty".into());
        }
        Ok(Self {
            description: description.into(),
            id,
            tags: Vec::new(),
            language,
            filename,
            source: source.into(),
            project: None,
            tools: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn with_project(mut self, project: ProjectCase) -> Self {
        self.project = Some(project);
        self
    }

    pub fn with_tool(
        mut self,
        name: impl Into<String>,
        expectation: ToolExpectation,
    ) -> Result<Self, String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("case tool name must not be empty".into());
        }
        self.tools.insert(name, expectation);
        Ok(self)
    }
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
    pub(crate) config: Option<String>,
    /// Explicit rule IDs to enable.
    pub(crate) rules: Vec<String>,
    /// Findings that must be present.
    pub(crate) required: Vec<DiagnosticExpectation>,
    /// Findings that must be absent.
    pub(crate) forbidden: Vec<DiagnosticExpectation>,
}

impl ToolExpectation {
    /// Construct an expectation after validating its mutually exclusive rule
    /// and config selectors.
    pub fn new(config: Option<String>, rules: Vec<String>) -> Result<Self, String> {
        if config.is_some() != rules.is_empty() {
            return Err("tool expectation must specify exactly one of config or rules".into());
        }
        Ok(Self {
            config,
            rules,
            required: Vec::new(),
            forbidden: Vec::new(),
        })
    }

    /// Construct an expectation with its complete diagnostic lists checked.
    pub fn from_parts(
        config: Option<String>,
        rules: Vec<String>,
        required: Vec<DiagnosticExpectation>,
        forbidden: Vec<DiagnosticExpectation>,
    ) -> Result<Self, String> {
        let mut expectation = Self::new(config, rules)?;
        expectation.required = required;
        expectation.forbidden = forbidden;
        Ok(expectation)
    }
}

#[derive(Clone, Debug)]
pub struct DiagnosticExpectation {
    /// Optional project-relative finding path.
    pub(crate) path: Option<String>,
    /// Stable rule ID to compare.
    pub(crate) rule_id: String,
    /// Optional message ID constraint.
    pub(crate) message_id: Option<String>,
    /// Optional severity constraint.
    pub(crate) severity: Option<Severity>,
    /// Exact expected count when specified.
    pub(crate) count: Option<usize>,
    /// Optional one-based source line.
    pub(crate) line: Option<u32>,
    /// Optional one-based source column.
    pub(crate) column: Option<u32>,
    /// Optional rendered-message constraint.
    pub(crate) message: Option<String>,
}

impl DiagnosticExpectation {
    /// Construct a required or forbidden diagnostic with a validated rule ID.
    pub fn new(rule_id: impl Into<String>) -> Result<Self, String> {
        let rule_id = rule_id.into();
        if rule_id.trim().is_empty() {
            return Err("diagnostic expectation rule ID must not be empty".into());
        }
        Ok(Self {
            path: None,
            rule_id,
            message_id: None,
            severity: None,
            count: Some(1),
            line: None,
            column: None,
            message: None,
        })
    }

    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    #[must_use]
    pub fn with_message_id(mut self, message_id: impl Into<String>) -> Self {
        self.message_id = Some(message_id.into());
        self
    }

    #[must_use]
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }

    #[must_use]
    pub fn with_count(mut self, count: Option<usize>) -> Self {
        self.count = count;
        self
    }

    #[must_use]
    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    #[must_use]
    pub fn with_column(mut self, column: u32) -> Self {
        self.column = Some(column);
        self
    }

    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
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
                path: ProjectRelativePath::new(path).map_err(|error| error.to_string())?,
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
}

#[derive(Clone, Debug)]
pub struct AdapterRun {
    /// Findings produced by one adapter invocation.
    pub findings: Vec<Finding>,
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
    /// Expectation mismatches between returned findings and fixture metadata.
    pub errors: Vec<String>,
    /// Failures while starting or executing the adapter, or decoding its
    /// response.
    pub operational_errors: Vec<String>,
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
            range: glass_lint_core::SourceRange::new(
                glass_lint_core::Position::new(1, 2).unwrap(),
                glass_lint_core::Position::new(1, 8).unwrap(),
            )
            .unwrap(),
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
