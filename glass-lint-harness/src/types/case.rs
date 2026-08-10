use std::collections::BTreeMap;

use glass_lint_core::{
    ProviderCatalogError, RuleId, Severity,
    project::{MatchCertainty, ProjectInputError, ProjectRelativePath},
};

use super::protocol::{AdapterFile, AdapterProject, AdapterResolution};

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum BundleProfile {
    Web,
    Obsidian,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum BundleTransformer {
    Vite,
    Esbuild,
}

impl BundleTransformer {
    #[must_use]
    pub const fn all() -> [Self; 2] {
        [Self::Vite, Self::Esbuild]
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vite => "vite",
            Self::Esbuild => "esbuild",
        }
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum BundleTarget {
    #[serde(rename = "ES5")]
    Es5,
    #[serde(rename = "ES6")]
    Es6,
    #[serde(rename = "ES2017")]
    Es2017,
    #[serde(rename = "ES2022")]
    Es2022,
    #[serde(rename = "ESNEXT")]
    Esnext,
}

impl BundleTarget {
    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::Es5,
            Self::Es6,
            Self::Es2017,
            Self::Es2022,
            Self::Esnext,
        ]
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Es5 => "ES5",
            Self::Es6 => "ES6",
            Self::Es2017 => "ES2017",
            Self::Es2022 => "ES2022",
            Self::Esnext => "ESNEXT",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
pub struct BundleKey {
    pub profile: BundleProfile,
    pub transformer: BundleTransformer,
    pub minified: bool,
    pub target: BundleTarget,
}

impl BundleKey {
    #[must_use]
    pub fn label(&self) -> String {
        format!(
            "{}/{}/minified={}/target={}",
            self.profile.as_str(),
            self.transformer.as_str(),
            self.minified,
            self.target.as_str()
        )
    }
}

impl BundleProfile {
    pub fn parse(value: &str) -> Result<Self, BundleProfileError> {
        match value {
            "web" => Ok(Self::Web),
            "obsidian" => Ok(Self::Obsidian),
            _ => Err(BundleProfileError::Unknown(value.to_owned())),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Obsidian => "obsidian",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BundleProfileError {
    Empty,
    Unknown(String),
    Duplicate(BundleProfile),
}

impl std::fmt::Display for BundleProfileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("@bundle must specify at least one profile"),
            Self::Unknown(profile) => write!(formatter, "unknown bundle profile `{profile}`"),
            Self::Duplicate(profile) => {
                write!(formatter, "duplicate bundle profile `{}`", profile.as_str())
            }
        }
    }
}

impl std::error::Error for BundleProfileError {}

pub fn normalize_bundle_profiles(
    values: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<Vec<BundleProfile>, BundleProfileError> {
    let mut profiles = Vec::new();
    for value in values {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(BundleProfileError::Empty);
        }
        let profile = BundleProfile::parse(value)?;
        if profiles.contains(&profile) {
            return Err(BundleProfileError::Duplicate(profile));
        }
        profiles.push(profile);
    }
    profiles.sort_unstable();
    Ok(profiles)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaseError {
    EmptyIdentity,
    EmptyToolName,
    DuplicateBundleDirective,
    BundledCaseNeedsGlassLint,
}

impl std::fmt::Display for CaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentity => {
                formatter.write_str("case id, language, and filename must not be empty")
            }
            Self::EmptyToolName => formatter.write_str("case tool name must not be empty"),
            Self::DuplicateBundleDirective => {
                formatter.write_str("a case may contain only one @bundle directive")
            }
            Self::BundledCaseNeedsGlassLint => {
                formatter.write_str("bundled cases must configure a `glass-lint` tool")
            }
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
            Self::InvalidRuleId(error) => write!(formatter, "invalid expectation rule ID: {error}"),
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

#[derive(Clone, Debug)]
/// One source fixture and its per-adapter expectations.
pub struct Case {
    pub(crate) id: String,
    pub(crate) description: String,
    pub(crate) tags: Vec<String>,
    pub(crate) language: String,
    pub(crate) filename: String,
    pub(crate) source: String,
    pub(crate) project: Option<ProjectCase>,
    pub(crate) adapters: BTreeMap<String, ToolExpectation>,
    pub(crate) bundles: Vec<BundleProfile>,
}

impl Case {
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
            bundles: Vec::new(),
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

    #[must_use]
    pub fn bundles(&self) -> &[BundleProfile] {
        &self.bundles
    }

    pub(crate) fn set_bundles(&mut self, bundles: Vec<BundleProfile>) {
        self.bundles = bundles;
    }

    pub(crate) fn validate_bundle_tool(&self) -> Result<(), CaseError> {
        if !self.bundles.is_empty() && !self.adapters.contains_key("glass-lint") {
            return Err(CaseError::BundledCaseNeedsGlassLint);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ProjectCase {
    pub(crate) protocol: AdapterProject,
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
pub struct ToolExpectation {
    selector: ToolSelector,
    required: Vec<FindingExpectation>,
    forbidden: Vec<FindingExpectation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolSelector {
    Config(String),
    Rules(Vec<String>),
}

impl ToolExpectation {
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

    pub(crate) fn required(&self) -> &[FindingExpectation] {
        &self.required
    }

    pub(crate) fn forbidden(&self) -> &[FindingExpectation] {
        &self.forbidden
    }

    pub(crate) fn qualify_for_file(mut self, path: &str) -> Result<Self, ProjectInputError> {
        self.required = qualify_findings(self.required, path)?;
        self.forbidden = qualify_findings(self.forbidden, path)?;
        Ok(self)
    }

    pub(crate) fn add_required(&mut self, finding: FindingExpectation) {
        self.required.push(finding);
    }

    pub(crate) fn add_forbidden(&mut self, finding: FindingExpectation) {
        self.forbidden.push(finding);
    }
}

fn qualify_findings(
    findings: Vec<FindingExpectation>,
    path: &str,
) -> Result<Vec<FindingExpectation>, ProjectInputError> {
    findings
        .into_iter()
        .map(|finding| finding.qualify_for_file(path))
        .collect()
}

#[derive(Clone, Debug)]
pub struct FindingExpectation {
    pub(crate) path: Option<ProjectRelativePath>,
    pub(crate) rule_id: RuleId,
    pub(crate) severity: Option<Severity>,
    pub(crate) count: ExpectedCount,
    pub(crate) line: Option<u32>,
    pub(crate) column: Option<u32>,
    pub(crate) message: Option<String>,
    pub(crate) certainty: Option<MatchCertainty>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedCount {
    Exactly(usize),
    AtLeastOne,
}

impl FindingExpectation {
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

    pub(crate) fn qualify_for_file(mut self, path: &str) -> Result<Self, ProjectInputError> {
        if self.path.is_none() {
            self.path = Some(ProjectRelativePath::new(path)?);
        }
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
