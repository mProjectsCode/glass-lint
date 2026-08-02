use std::collections::BTreeMap;

use glass_lint_core::{
    ProviderCatalogError, RuleId, Severity,
    project::{MatchCertainty, ProjectInputError, ProjectRelativePath},
};

use super::protocol::{AdapterFile, AdapterProject, AdapterResolution};

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
        self.required = self
            .required
            .into_iter()
            .map(|finding| finding.qualify_for_file(path))
            .collect::<Result<Vec<_>, _>>()?;
        self.forbidden = self
            .forbidden
            .into_iter()
            .map(|finding| finding.qualify_for_file(path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self)
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
