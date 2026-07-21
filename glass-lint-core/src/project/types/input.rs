use std::path::PathBuf;

use crate::{SourceLanguage, project::types::ProjectRelativePath};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct SourceFile {
    pub path: ProjectRelativePath,
    pub language: SourceLanguage,
    pub source: String,
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
            source: source.into(),
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
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
    Unsupported { reason: String },
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
        }
    }
}

impl std::error::Error for ProjectInputError {}
