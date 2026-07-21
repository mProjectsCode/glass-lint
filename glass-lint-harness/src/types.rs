//! Case, adapter-protocol, result, and profiling data contracts.

use std::collections::BTreeMap;

use glass_lint_core::{
    Finding, ProjectRelativePath, ResolutionRequestKind, ResolverOutcome, RuleId, Severity,
};
use serde::{Deserialize, Serialize};

pub const ADAPTER_PROTOCOL_VERSION: u32 = 3;

#[derive(Clone, Debug)]
/// One source fixture and its per-adapter expectations.
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
    /// Optional multi-file project file.
    pub(crate) project: Option<ProjectCase>,
    /// Expectations keyed by adapter name.
    pub(crate) adapters: BTreeMap<String, ToolExpectation>,
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
            adapters: BTreeMap::new(),
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
        self.adapters.insert(name, expectation);
        Ok(self)
    }
}

/// A multi-file harness file. Paths are project-relative and sources are
/// retained in sorted order so virtual and filesystem cases have identical
/// identities.
#[derive(Clone, Debug)]
pub struct ProjectCase {
    /// Canonical adapter-boundary project contract.
    pub(crate) protocol: AdapterProject,
    /// Whether the adapter should load the real filesystem tree.
    pub filesystem: bool,
}

impl ProjectCase {
    pub(crate) fn root(&self) -> std::path::PathBuf {
        self.protocol.root.clone().into()
    }

    pub(crate) fn files(&self) -> &[AdapterFile] {
        &self.protocol.files
    }

    pub(crate) fn resolutions(&self) -> &[AdapterResolution] {
        &self.protocol.resolutions
    }
}

impl From<&ProjectCase> for AdapterProject {
    fn from(project: &ProjectCase) -> Self {
        project.protocol.clone()
    }
}
impl From<AdapterProject> for ProjectCase {
    fn from(project: AdapterProject) -> Self {
        Self {
            protocol: project,
            filesystem: false,
        }
    }
}

#[derive(Clone, Debug)]
/// Expectations for one adapter on one case.
pub struct ToolExpectation {
    /// The mutually exclusive adapter selector.
    selector: ToolSelector,
    /// Findings that must be present.
    required: Vec<FindingExpectation>,
    /// Findings that must be absent.
    forbidden: Vec<FindingExpectation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolSelector {
    /// Named adapter configuration.
    Config(String),
    /// Explicit, non-empty rule IDs to enable.
    Rules(Vec<String>),
}

impl ToolExpectation {
    /// Construct an expectation after validating its mutually exclusive rule
    /// and config selectors.
    pub fn new(config: Option<String>, rules: Vec<String>) -> Result<Self, String> {
        let selector = match (config, rules) {
            (Some(config), rules) if !config.trim().is_empty() && rules.is_empty() => {
                ToolSelector::Config(config)
            }
            (None, rules) if !rules.is_empty() => ToolSelector::Rules(rules),
            _ => return Err("tool expectation must specify exactly one of config or rules".into()),
        };
        Ok(Self {
            selector,
            required: Vec::new(),
            forbidden: Vec::new(),
        })
    }

    pub(crate) fn config(&self) -> Option<&str> {
        match &self.selector {
            ToolSelector::Config(config) => Some(config),
            ToolSelector::Rules(_) => None,
        }
    }

    pub(crate) fn rules(&self) -> &[String] {
        match &self.selector {
            ToolSelector::Config(_) => &[],
            ToolSelector::Rules(rules) => rules,
        }
    }

    pub(crate) fn merge_from(&mut self, other: Self) -> Result<(), String> {
        if self.selector != other.selector {
            return Err("tool expectation selectors disagree across project files".into());
        }
        self.required.extend(other.required);
        self.forbidden.extend(other.forbidden);
        Ok(())
    }

    /// Construct an expectation with its complete diagnostic lists checked.
    pub fn from_parts(
        config: Option<String>,
        rules: Vec<String>,
        required: Vec<FindingExpectation>,
        forbidden: Vec<FindingExpectation>,
    ) -> Result<Self, String> {
        let mut expectation = Self::new(config, rules)?;
        expectation.required = required;
        expectation.forbidden = forbidden;
        Ok(expectation)
    }

    pub(crate) fn from_selector(
        selector: ToolSelector,
        required: Vec<FindingExpectation>,
        forbidden: Vec<FindingExpectation>,
    ) -> Result<Self, String> {
        let valid = match &selector {
            ToolSelector::Config(config) => !config.trim().is_empty(),
            ToolSelector::Rules(rules) => !rules.is_empty(),
        };
        if !valid {
            return Err("tool expectation selector is invalid".into());
        }
        Ok(Self {
            selector,
            required,
            forbidden,
        })
    }

    pub(crate) fn required(&self) -> &[FindingExpectation] {
        &self.required
    }

    pub(crate) fn forbidden(&self) -> &[FindingExpectation] {
        &self.forbidden
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        ToolSelector,
        Vec<FindingExpectation>,
        Vec<FindingExpectation>,
    ) {
        (self.selector, self.required, self.forbidden)
    }

    pub(crate) fn add_required(&mut self, finding: FindingExpectation) {
        self.required.push(finding);
    }

    pub(crate) fn add_forbidden(&mut self, finding: FindingExpectation) {
        self.forbidden.push(finding);
    }
}

#[derive(Clone, Debug)]
pub struct FindingExpectation {
    /// Optional project-relative finding path.
    pub(crate) path: Option<ProjectRelativePath>,
    /// Stable rule ID to compare.
    pub(crate) rule_id: RuleId,
    /// Optional message ID constraint.
    pub(crate) message_id: Option<String>,
    /// Optional severity constraint.
    pub(crate) severity: Option<Severity>,
    /// Exact expected count when specified.
    pub(crate) count: ExpectedCount,
    /// Optional one-based source line.
    pub(crate) line: Option<u32>,
    /// Optional one-based source column.
    pub(crate) column: Option<u32>,
    /// Optional rendered-message constraint.
    pub(crate) message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedCount {
    /// Require exactly this many matching findings.
    Exactly(usize),
    /// Require at least one matching finding.
    AtLeastOne,
}

impl FindingExpectation {
    /// Construct a required or forbidden diagnostic with a validated rule ID.
    pub fn new(rule_id: impl Into<String>) -> Result<Self, String> {
        let rule_id = RuleId::parse(rule_id.into())
            .map_err(|_| "diagnostic expectation rule ID is invalid".to_owned())?;
        Ok(Self {
            path: None,
            rule_id,
            message_id: None,
            severity: None,
            count: ExpectedCount::Exactly(1),
            line: None,
            column: None,
            message: None,
        })
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Result<Self, String> {
        self.path = Some(ProjectRelativePath::new(path.into()).map_err(|error| error.to_string())?);
        Ok(self)
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
    pub fn with_count(mut self, count: ExpectedCount) -> Self {
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
impl TryFrom<&AdapterResolution> for (ResolutionRequestKind, ResolverOutcome) {
    type Error = String;

    fn try_from(resolution: &AdapterResolution) -> Result<Self, Self::Error> {
        let kind = match resolution.kind {
            AdapterResolutionKind::Import => ResolutionRequestKind::StaticImport,
            AdapterResolutionKind::DynamicImport => ResolutionRequestKind::DynamicImport,
            AdapterResolutionKind::Require => ResolutionRequestKind::Require,
        };
        let result = match &resolution.result {
            AdapterResolutionResult::Internal { path } => ResolverOutcome::Internal {
                path: ProjectRelativePath::new(path).map_err(|error| error.to_string())?,
            },
            AdapterResolutionResult::External { package } => ResolverOutcome::External {
                package: package.clone(),
            },
            AdapterResolutionResult::Builtin { name } => {
                ResolverOutcome::Builtin { name: name.clone() }
            }
            AdapterResolutionResult::Missing => ResolverOutcome::Missing,
            AdapterResolutionResult::OutsideProject { path } => {
                ResolverOutcome::OutsideProject { path: path.clone() }
            }
            AdapterResolutionResult::Unsupported { reason } => ResolverOutcome::Unsupported {
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
    pub adapters: BTreeMap<String, ToolResult>,
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
    pub mismatches: Vec<String>,
    /// Failures while starting or executing the adapter, or decoding its
    /// response.
    pub operational_errors: Vec<String>,
}

impl ToolResult {
    #[must_use]
    pub fn skipped(version: String, skip_reason: Option<String>) -> Self {
        Self {
            version,
            skipped: true,
            skip_reason,
            passed: true,
            findings: vec![],
            mismatches: vec![],
            operational_errors: vec![],
        }
    }
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
            .all(|case| case.adapters.values().all(|adapter| adapter.passed))
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
