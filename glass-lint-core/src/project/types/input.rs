use std::{borrow::Borrow, ops::Deref, sync::Arc};

use smol_str::SmolStr;

use crate::{SourceLanguage, project::types::ProjectRelativePath};

/// Shared source text admitted once at the project boundary.
///
/// The public project DTO still serializes as a string, but every internal
/// consumer clones only this cheap handle instead of copying the source.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

/// A validated package specifier (e.g., "lodash", "@angular/core").
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageSpecifier(SmolStr);

impl PackageSpecifier {
    pub fn new(s: impl Into<SmolStr>) -> Result<Self, ProjectInputError> {
        let inner = s.into();
        if inner.trim().is_empty() {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }
        Ok(Self(inner))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for PackageSpecifier {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for PackageSpecifier {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for PackageSpecifier {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PackageSpecifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<&str> for PackageSpecifier {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// A validated builtin module name (e.g., "node:fs", "node:path",
/// "node:buffer").
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuiltinModuleName(SmolStr);

impl BuiltinModuleName {
    pub fn new(s: impl Into<SmolStr>) -> Result<Self, ProjectInputError> {
        let inner = s.into();
        if inner.trim().is_empty() {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }
        Ok(Self(inner))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for BuiltinModuleName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for BuiltinModuleName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for BuiltinModuleName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BuiltinModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<&str> for BuiltinModuleName {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// A normalized outside-project path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NormalizedOutsidePath(SmolStr);

impl NormalizedOutsidePath {
    pub fn new(s: impl Into<SmolStr>) -> Result<Self, ProjectInputError> {
        let inner = s.into();
        if inner.trim().is_empty() {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }
        Ok(Self(inner))
    }

    pub(crate) fn from_validated(inner: impl Into<SmolStr>) -> Self {
        Self(inner.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for NormalizedOutsidePath {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for NormalizedOutsidePath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for NormalizedOutsidePath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for NormalizedOutsidePath {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl std::fmt::Display for NormalizedOutsidePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<std::path::Path> for NormalizedOutsidePath {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.0)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SourceFile {
    path: ProjectRelativePath,
    language: SourceLanguage,
    source: SourceText,
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

    /// Construct from a validated project-relative path without re-parsing.
    pub fn from_relative(path: ProjectRelativePath, source: impl Into<SourceText>) -> Self {
        let language = SourceLanguage::from_filename(&path);
        Self {
            path,
            language,
            source: source.into(),
        }
    }

    pub fn path(&self) -> &ProjectRelativePath {
        &self.path
    }

    pub fn language(&self) -> SourceLanguage {
        self.language
    }

    pub fn source(&self) -> &SourceText {
        &self.source
    }

    pub fn into_path(self) -> ProjectRelativePath {
        self.path
    }

    pub fn into_source(self) -> SourceText {
        self.source
    }

    pub(crate) fn set_path(&mut self, path: ProjectRelativePath) {
        self.path = path;
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResolutionRequestKind {
    StaticImport,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResolutionRequestKey {
    pub importer: ProjectRelativePath,
    pub kind: ResolutionRequestKind,
    pub range: glass_lint_datastructures::SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionRequest {
    pub key: ResolutionRequestKey,
    pub request: SmolStr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolverOutcome {
    Internal { path: ProjectRelativePath },
    External { package: PackageSpecifier },
    Builtin { name: BuiltinModuleName },
    Missing,
    OutsideProject { path: NormalizedOutsidePath },
    Unsupported { reason: String },
}

/// Stable opaque identity assigned from normalized project path order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModuleId(u32);

impl ModuleId {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkedModuleTarget {
    Internal {
        id: ModuleId,
        path: ProjectRelativePath,
    },
    External {
        package: PackageSpecifier,
    },
    Builtin {
        name: BuiltinModuleName,
    },
    Missing,
    OutsideProject {
        path: NormalizedOutsidePath,
    },
    Unsupported {
        reason: String,
    },
}

/// Errors from local job execution (worker panic, channel failure, etc.).
/// Parse failures are returned as ordinary per-job results, not through this
/// type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalExecutionError {
    /// A worker thread panicked during local analysis.
    WorkerPanic,
}

impl std::fmt::Display for LocalExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkerPanic => write!(f, "analysis worker panicked"),
        }
    }
}

impl std::error::Error for LocalExecutionError {}

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
    LocalExecution(LocalExecutionError),
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
            Self::LocalExecution(error) => {
                write!(f, "local analysis execution failed: {error}")
            }
        }
    }
}

impl std::error::Error for ProjectInputError {}
