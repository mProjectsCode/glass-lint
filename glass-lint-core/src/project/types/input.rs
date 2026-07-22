use std::{ops::Deref, path::PathBuf, sync::Arc};

use crate::{SourceLanguage, project::types::ProjectRelativePath};

/// Shared source text admitted once at the project boundary.
///
/// The public project DTO still serializes as a string, but every internal
/// consumer clones only this cheap handle instead of copying the source.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct SourceText(Arc<str>);

impl SourceText {
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        Self(source.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for SourceText {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl From<String> for SourceText {
    fn from(source: String) -> Self {
        Self::new(Arc::<str>::from(source))
    }
}

impl From<&str> for SourceText {
    fn from(source: &str) -> Self {
        Self::new(Arc::<str>::from(source))
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct SourceFile {
    pub path: ProjectRelativePath,
    pub language: SourceLanguage,
    pub source: SourceText,
}

impl SourceFile {
    pub fn new(
        path: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, ProjectInputError> {
        let path = path.into();
        let path = ProjectRelativePath::new(&path)?;
        Ok(Self {
            language: SourceLanguage::from_filename(&path),
            path,
            source: source.into().into(),
        })
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum ResolutionRequestKind {
    StaticImport,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct ResolutionRequestKey {
    pub importer: ProjectRelativePath,
    pub kind: ResolutionRequestKind,
    pub range: crate::SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ResolutionRequest {
    pub key: ResolutionRequestKey,
    pub request: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ResolverOutcome {
    Internal { path: ProjectRelativePath },
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
    Unsupported { reason: String },
}

/// Stable opaque identity assigned from normalized project path order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
pub struct ModuleId(u32);

impl ModuleId {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum LinkedModuleTarget {
    Internal {
        id: ModuleId,
        path: ProjectRelativePath,
    },
    External {
        package: String,
    },
    Builtin {
        name: String,
    },
    Missing,
    OutsideProject {
        path: String,
    },
    Unsupported {
        reason: String,
    },
}

/// Unvalidated caller-supplied project sources and resolver answers.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectInput {
    pub root: PathBuf,
    pub sources: Vec<SourceFile>,
    pub resolutions: Vec<(ResolutionRequestKey, ResolverOutcome)>,
}

/// Validation failures for project inputs and explicit resolver answers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectInputError {
    InvalidPath(String),
    DuplicateSource(String),
    UnknownImporter(String),
    DuplicateResolution(ResolutionRequestKey),
    InvalidTarget(String),
    UnknownRequest(ResolutionRequestKey),
    BudgetExceeded(String),
    LocalExecution(String),
}

impl std::fmt::Display for ProjectInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(path) => write!(f, "invalid project path `{path}`"),
            Self::DuplicateSource(path) => write!(f, "duplicate project source `{path}`"),
            Self::UnknownImporter(path) => {
                write!(f, "resolution importer is not a source: `{path}`")
            }
            Self::DuplicateResolution(key) => {
                write!(f, "duplicate resolution for `{}`", key.importer)
            }
            Self::InvalidTarget(path) => write!(f, "invalid resolution target `{path}`"),
            Self::UnknownRequest(key) => write!(
                f,
                "resolution does not match an authored request in `{}`",
                key.importer
            ),
            Self::BudgetExceeded(message) => write!(f, "project input budget exceeded: {message}"),
            Self::LocalExecution(message) => {
                write!(f, "local analysis execution failed: {message}")
            }
        }
    }
}

impl std::error::Error for ProjectInputError {}
