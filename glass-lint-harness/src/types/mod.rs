//! Case, adapter-protocol, result, and profiling data contracts.

use std::collections::BTreeMap;

use glass_lint_core::{
    ProviderCatalogError, RuleId, Severity,
    project::{
        BuiltinModuleName, EvidenceRole, EvidenceStep, EvidenceTrace, EvidenceTraces, Finding,
        MatchCertainty, NormalizedOutsidePath, PackageSpecifier, ProjectInputError,
        ProjectRelativePath, ResolutionRequestKind, ResolverOutcome, SourceLocation,
    },
};
use serde::{Deserialize, Serialize};

pub const ADAPTER_PROTOCOL_VERSION: u32 = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaseError {
    EmptyIdentity,
    EmptyToolName,
}

impl std::fmt::Display for CaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentity => {
                formatter.write_str("case id, language, and filename must not be empty")
            }
            Self::EmptyToolName => formatter.write_str("case tool name must not be empty"),
        }
    }
}

impl std::error::Error for CaseError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpectationError {
    InvalidSelector,
    SelectorMismatch,
}

impl std::fmt::Display for ExpectationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSelector => {
                formatter.write_str("tool expectation must specify exactly one of config or rules")
            }
            Self::SelectorMismatch => {
                formatter.write_str("tool expectation selectors disagree across project files")
            }
        }
    }
}

impl std::error::Error for ExpectationError {}

#[derive(Debug)]
pub enum FindingExpectationError {
    InvalidRuleId(ProviderCatalogError),
    InvalidPath(ProjectInputError),
}

impl std::fmt::Display for FindingExpectationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRuleId(error) => {
                write!(formatter, "invalid expectation rule ID: {error}")
            }
            Self::InvalidPath(error) => write!(formatter, "invalid expectation path: {error}"),
        }
    }
}

impl std::error::Error for FindingExpectationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidRuleId(error) => Some(error),
            Self::InvalidPath(error) => Some(error),
        }
    }
}

#[derive(Debug)]
pub enum AdapterConversionError {
    InvalidInternalPath(ProjectInputError),
    InvalidPackage(ProjectInputError),
    InvalidBuiltin(ProjectInputError),
    InvalidOutsideProjectPath(ProjectInputError),
}

impl std::fmt::Display for AdapterConversionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInternalPath(error) => write!(formatter, "invalid internal path: {error}"),
            Self::InvalidPackage(error) => write!(formatter, "invalid package: {error}"),
            Self::InvalidBuiltin(error) => write!(formatter, "invalid builtin: {error}"),
            Self::InvalidOutsideProjectPath(error) => {
                write!(formatter, "invalid outside-project path: {error}")
            }
        }
    }
}

impl std::error::Error for AdapterConversionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidInternalPath(error)
            | Self::InvalidPackage(error)
            | Self::InvalidBuiltin(error)
            | Self::InvalidOutsideProjectPath(error) => Some(error),
        }
    }
}

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
    ) -> Result<Self, CaseError> {
        let id = id.into();
        let language = language.into();
        let filename = filename.into();
        if id.trim().is_empty() || language.trim().is_empty() || filename.trim().is_empty() {
            return Err(CaseError::EmptyIdentity);
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
    ) -> Result<Self, CaseError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(CaseError::EmptyToolName);
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
    pub fn new(config: Option<String>, rules: Vec<String>) -> Result<Self, ExpectationError> {
        let selector = match (config, rules) {
            (Some(config), rules) if !config.trim().is_empty() && rules.is_empty() => {
                ToolSelector::Config(config)
            }
            (None, rules) if !rules.is_empty() => ToolSelector::Rules(rules),
            _ => return Err(ExpectationError::InvalidSelector),
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

    pub(crate) fn merge_from(&mut self, other: Self) -> Result<(), ExpectationError> {
        if self.selector != other.selector {
            return Err(ExpectationError::SelectorMismatch);
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
    ) -> Result<Self, ExpectationError> {
        let mut expectation = Self::new(config, rules)?;
        expectation.required = required;
        expectation.forbidden = forbidden;
        Ok(expectation)
    }

    pub(crate) fn from_selector(
        selector: ToolSelector,
        required: Vec<FindingExpectation>,
        forbidden: Vec<FindingExpectation>,
    ) -> Result<Self, ExpectationError> {
        let valid = match &selector {
            ToolSelector::Config(config) => !config.trim().is_empty(),
            ToolSelector::Rules(rules) => !rules.is_empty(),
        };
        if !valid {
            return Err(ExpectationError::InvalidSelector);
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
    /// Optional match certainty constraint.
    pub(crate) certainty: Option<MatchCertainty>,
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
    pub fn new(rule_id: impl Into<String>) -> Result<Self, FindingExpectationError> {
        let rule_id =
            RuleId::parse(rule_id.into()).map_err(FindingExpectationError::InvalidRuleId)?;
        Ok(Self {
            path: None,
            rule_id,
            severity: None,
            count: ExpectedCount::Exactly(1),
            line: None,
            column: None,
            message: None,
            certainty: None,
        })
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Result<Self, FindingExpectationError> {
        self.path = Some(
            ProjectRelativePath::new(path.into()).map_err(FindingExpectationError::InvalidPath)?,
        );
        Ok(self)
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

    #[must_use]
    pub fn with_certainty(mut self, certainty: MatchCertainty) -> Self {
        self.certainty = Some(certainty);
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
    pub range: glass_lint_datastructures::SourceRange,
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
    type Error = AdapterConversionError;

    fn try_from(resolution: &AdapterResolution) -> Result<Self, Self::Error> {
        let kind = match resolution.kind {
            AdapterResolutionKind::Import => ResolutionRequestKind::StaticImport,
            AdapterResolutionKind::DynamicImport => ResolutionRequestKind::DynamicImport,
            AdapterResolutionKind::Require => ResolutionRequestKind::Require,
        };
        let result = match &resolution.result {
            AdapterResolutionResult::Internal { path } => ResolverOutcome::Internal {
                path: ProjectRelativePath::new(path)
                    .map_err(AdapterConversionError::InvalidInternalPath)?,
            },
            AdapterResolutionResult::External { package } => ResolverOutcome::External {
                package: PackageSpecifier::new(package.clone())
                    .map_err(AdapterConversionError::InvalidPackage)?,
            },
            AdapterResolutionResult::Builtin { name } => ResolverOutcome::Builtin {
                name: BuiltinModuleName::new(name.clone())
                    .map_err(AdapterConversionError::InvalidBuiltin)?,
            },
            AdapterResolutionResult::Missing => ResolverOutcome::Missing,
            AdapterResolutionResult::OutsideProject { path } => ResolverOutcome::OutsideProject {
                path: NormalizedOutsidePath::new(path.clone())
                    .map_err(AdapterConversionError::InvalidOutsideProjectPath)?,
            },
            AdapterResolutionResult::Unsupported { reason } => ResolverOutcome::Unsupported {
                reason: reason.clone(),
            },
        };
        Ok((kind, result))
    }
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AdapterResponseDto {
    protocol_version: u32,
    tool: String,
    tool_version: String,
    findings: Vec<AdapterFindingDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AdapterFindingDto {
    rule_id: String,
    message: String,
    severity: Severity,
    location: AdapterSourceLocation,
    certainty: MatchCertainty,
    evidence: AdapterEvidenceDto,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AdapterSourceLocation {
    path: String,
    range: glass_lint_datastructures::SourceRange,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AdapterEvidenceDto {
    traces: Vec<AdapterTraceDto>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AdapterTraceDto {
    steps: Vec<AdapterStepDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AdapterStepDto {
    role: EvidenceRole,
    message: String,
    location: AdapterSourceLocation,
}

impl From<&SourceLocation> for AdapterSourceLocation {
    fn from(location: &SourceLocation) -> Self {
        Self {
            path: location.path().as_str().to_owned(),
            range: location.range(),
        }
    }
}

impl From<&Finding> for AdapterFindingDto {
    fn from(finding: &Finding) -> Self {
        Self {
            rule_id: finding.rule_id().to_string(),
            message: finding.message().to_owned(),
            severity: finding.severity(),
            location: finding.location().into(),
            certainty: finding.certainty(),
            evidence: AdapterEvidenceDto {
                traces: finding
                    .evidence()
                    .traces()
                    .iter()
                    .map(|trace| AdapterTraceDto {
                        steps: trace
                            .steps()
                            .iter()
                            .map(|step| AdapterStepDto {
                                role: step.role(),
                                message: step.message().to_owned(),
                                location: step.location().into(),
                            })
                            .collect(),
                    })
                    .collect(),
                truncated: finding.evidence().truncated(),
            },
        }
    }
}

impl From<&AdapterResponse> for AdapterResponseDto {
    fn from(response: &AdapterResponse) -> Self {
        Self {
            protocol_version: response.protocol_version,
            tool: response.tool.clone(),
            tool_version: response.tool_version.clone(),
            findings: response.findings.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug)]
enum AdapterFindingError {
    InvalidRuleId(ProviderCatalogError),
    InvalidPath(ProjectInputError),
    EmptyEvidence,
    EmptyTrace,
    TraceDoesNotEndAtFinding,
}

impl std::fmt::Display for AdapterFindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRuleId(error) => write!(formatter, "invalid rule ID: {error}"),
            Self::InvalidPath(error) => write!(formatter, "invalid finding path: {error}"),
            Self::EmptyEvidence => {
                formatter.write_str("adapter finding evidence must contain at least one trace")
            }
            Self::EmptyTrace => {
                formatter.write_str("adapter evidence traces must contain at least one step")
            }
            Self::TraceDoesNotEndAtFinding => {
                formatter.write_str("adapter evidence trace must end at the finding location")
            }
        }
    }
}

impl std::error::Error for AdapterFindingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidRuleId(error) => Some(error),
            Self::InvalidPath(error) => Some(error),
            Self::EmptyEvidence | Self::EmptyTrace | Self::TraceDoesNotEndAtFinding => None,
        }
    }
}

impl TryFrom<AdapterFindingDto> for Finding {
    type Error = AdapterFindingError;

    fn try_from(finding: AdapterFindingDto) -> Result<Self, Self::Error> {
        let path = ProjectRelativePath::new(finding.location.path)
            .map_err(AdapterFindingError::InvalidPath)?;
        let location = SourceLocation::new(path, finding.location.range);
        if finding.evidence.traces.is_empty() {
            return Err(AdapterFindingError::EmptyEvidence);
        }
        let traces = finding
            .evidence
            .traces
            .into_iter()
            .map(|trace| {
                if trace.steps.is_empty() {
                    return Err(AdapterFindingError::EmptyTrace);
                }
                let steps = trace
                    .steps
                    .into_iter()
                    .map(|step| {
                        let path = ProjectRelativePath::new(step.location.path)
                            .map_err(AdapterFindingError::InvalidPath)?;
                        Ok(EvidenceStep::new(
                            step.role,
                            step.message,
                            SourceLocation::new(path, step.location.range),
                        ))
                    })
                    .collect::<Result<Vec<_>, AdapterFindingError>>()?;
                Ok(EvidenceTrace::new(steps))
            })
            .collect::<Result<Vec<_>, AdapterFindingError>>()?;
        if traces.iter().any(|trace| {
            trace
                .steps()
                .last()
                .is_none_or(|step| step.location() != &location)
        }) {
            return Err(AdapterFindingError::TraceDoesNotEndAtFinding);
        }
        let rule_id = RuleId::parse(finding.rule_id).map_err(AdapterFindingError::InvalidRuleId)?;
        Ok(Self::new(
            rule_id,
            finding.message,
            finding.severity,
            location,
            EvidenceTraces::with_truncation(traces, finding.evidence.truncated),
            finding.certainty,
        ))
    }
}

impl serde::Serialize for AdapterResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        AdapterResponseDto::from(self).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for AdapterResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let response = AdapterResponseDto::deserialize(deserializer)?;
        let findings = response
            .findings
            .into_iter()
            .map(Finding::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(serde::de::Error::custom)?;
        Ok(Self {
            protocol_version: response.protocol_version,
            tool: response.tool,
            tool_version: response.tool_version,
            findings,
        })
    }
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
mod tests;
