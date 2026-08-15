use glass_lint_core::{
    ProviderCatalogError, RuleId, Severity,
    project::{
        BuiltinModuleName, EvidenceRole, EvidenceStep, EvidenceTrace, EvidenceTraces, Finding,
        MatchCertainty, NormalizedOutsidePath, PackageSpecifier, ProjectInputError,
        ProjectRelativePath, ResolutionRequestKind, ResolvedTargetKind, ResolverOutcome,
        SourceLocation,
    },
};
use serde::{Deserialize, Serialize};

pub const ADAPTER_PROTOCOL_VERSION: u32 = 4;

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
    pub range: glass_lint_datastructures::SourceRange,
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
            AdapterResolutionResult::External { package } => {
                ResolverOutcome::Target(ResolvedTargetKind::External {
                    package: PackageSpecifier::new(package.clone())
                        .map_err(AdapterConversionError::InvalidPackage)?,
                })
            }
            AdapterResolutionResult::Builtin { name } => {
                ResolverOutcome::Target(ResolvedTargetKind::Builtin {
                    name: BuiltinModuleName::new(name.clone())
                        .map_err(AdapterConversionError::InvalidBuiltin)?,
                })
            }
            AdapterResolutionResult::Missing => {
                ResolverOutcome::Target(ResolvedTargetKind::Missing)
            }
            AdapterResolutionResult::OutsideProject { path } => {
                ResolverOutcome::Target(ResolvedTargetKind::OutsideProject {
                    path: NormalizedOutsidePath::new(path.clone())
                        .map_err(AdapterConversionError::InvalidOutsideProjectPath)?,
                })
            }
            AdapterResolutionResult::Unsupported { reason } => {
                ResolverOutcome::Target(ResolvedTargetKind::Unsupported {
                    reason: reason.clone(),
                })
            }
        };
        Ok((kind, result))
    }
}

#[derive(Clone, Debug)]
pub struct AdapterResponse {
    pub protocol_version: u32,
    pub tool: String,
    pub tool_version: String,
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
            range: location.range_owned(),
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
                EvidenceTrace::new(steps).map_err(|_| AdapterFindingError::EmptyTrace)
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
        let evidence = if finding.evidence.truncated {
            EvidenceTraces::from_truncated(traces)
        } else {
            EvidenceTraces::new(traces).map_err(|_| AdapterFindingError::EmptyEvidence)?
        };
        Ok(Self::new(
            rule_id,
            finding.message,
            finding.severity,
            location,
            evidence,
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
