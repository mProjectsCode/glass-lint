use std::{borrow::Borrow, ops::Deref, sync::Arc};

use smol_str::SmolStr;

use crate::{SourceLanguage, project::types::ProjectRelativePath};

/// Shared source text accepted once at the project boundary.
///
/// The public project DTO still serializes as a string, but every internal
/// consumer clones only this cheap handle instead of copying the source.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
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

impl From<Arc<str>> for SourceText {
    fn from(source: Arc<str>) -> Self {
        Self(source)
    }
}

impl From<&str> for SourceText {
    fn from(source: &str) -> Self {
        Self::new(Arc::<str>::from(source))
    }
}

/// The canonical validated package-root value (e.g., "lodash",
/// "@angular/core") shared by project inputs and rule declarations.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageSpecifier(SmolStr);

impl PackageSpecifier {
    pub fn new(s: impl Into<SmolStr>) -> Result<Self, ProjectInputError> {
        let inner = s.into();
        let value = inner.trim();
        if value.is_empty()
            || value.contains('\0')
            || value.contains(char::is_whitespace)
            || value.starts_with('.')
            || value.starts_with('/')
            || value.starts_with('\\')
        {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }

        if let Some(scoped) = value.strip_prefix('@') {
            let mut segments = scoped.split('/');
            if segments.next().is_none_or(str::is_empty)
                || segments.next().is_none_or(str::is_empty)
                || segments.next().is_some()
            {
                return Err(ProjectInputError::InvalidTarget(inner.to_string()));
            }
        } else if value.contains('/') {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }

        Ok(Self(value.into()))
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
        let trimmed = inner.trim();
        if trimmed.is_empty() || trimmed.contains('\0') || trimmed.contains(char::is_whitespace) {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }
        if !trimmed.starts_with("node:") {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }
        let name = &trimmed[5..];
        if name.is_empty() || name.contains('\0') || name.contains(char::is_whitespace) {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }
        Ok(Self(trimmed.into()))
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
        let normalized = crate::project::input::normalize_outside_target(inner.as_str())?;
        Ok(Self(SmolStr::from(normalized)))
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
    /// Construct a virtual source using JavaScript parser semantics.
    /// Filename extensions do not select a language; filesystem acceptance
    /// supplies one explicitly through [`Self::with_language`].
    pub fn new(
        path: impl Into<String>,
        source: impl Into<SourceText>,
    ) -> Result<Self, ProjectInputError> {
        Self::with_language(path, source, SourceLanguage::JavaScript)
    }

    /// Construct a virtual source with an explicit parser language.
    pub fn with_language(
        path: impl Into<String>,
        source: impl Into<SourceText>,
        language: SourceLanguage,
    ) -> Result<Self, ProjectInputError> {
        let path = ProjectRelativePath::new(path.into())?;
        Ok(Self::from_parts(path, source.into(), language))
    }

    /// Construct a virtual source from a validated path using JavaScript
    /// parser semantics. Filesystem acceptance supplies an explicit language.
    pub fn from_relative(path: ProjectRelativePath, source: impl Into<SourceText>) -> Self {
        Self::from_relative_with_language(path, source, SourceLanguage::JavaScript)
    }

    /// Construct from a validated project-relative path with an explicit
    /// language, ignoring the filename extension.
    pub fn from_relative_with_language(
        path: ProjectRelativePath,
        source: impl Into<SourceText>,
        language: SourceLanguage,
    ) -> Self {
        Self::from_parts(path, source.into(), language)
    }

    fn from_parts(path: ProjectRelativePath, source: SourceText, language: SourceLanguage) -> Self {
        Self {
            path,
            language,
            source,
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
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResolutionRequestKind {
    StaticImport,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResolutionRequestKey {
    importer: ProjectRelativePath,
    kind: ResolutionRequestKind,
    range: glass_lint_datastructures::SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionRequest {
    key: ResolutionRequestKey,
    request: SmolStr,
}

impl ResolutionRequestKey {
    pub fn new(
        importer: ProjectRelativePath,
        kind: ResolutionRequestKind,
        range: glass_lint_datastructures::SourceRange,
    ) -> Self {
        Self {
            importer,
            kind,
            range,
        }
    }

    pub fn importer(&self) -> &ProjectRelativePath {
        &self.importer
    }

    pub fn kind(&self) -> ResolutionRequestKind {
        self.kind
    }

    pub fn range(&self) -> &glass_lint_datastructures::SourceRange {
        &self.range
    }

    pub fn range_owned(&self) -> glass_lint_datastructures::SourceRange {
        self.range.clone()
    }
}

impl ResolutionRequest {
    pub fn new(key: ResolutionRequestKey, specifier: impl Into<SmolStr>) -> Self {
        Self {
            key,
            request: specifier.into(),
        }
    }

    pub fn key(&self) -> &ResolutionRequestKey {
        &self.key
    }

    pub fn importer(&self) -> &ProjectRelativePath {
        self.key.importer()
    }

    pub fn kind(&self) -> ResolutionRequestKind {
        self.key.kind()
    }

    pub fn range(&self) -> &glass_lint_datastructures::SourceRange {
        self.key.range()
    }

    pub fn range_owned(&self) -> glass_lint_datastructures::SourceRange {
        self.key.range_owned()
    }

    pub fn specifier(&self) -> &SmolStr {
        &self.request
    }
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

impl ResolverOutcome {
    pub(crate) fn validate(self) -> Result<Self, ProjectPhaseError> {
        if let Self::Unsupported { reason } = &self
            && reason.trim().is_empty()
        {
            return Err(ProjectPhaseError::InvalidTarget(reason.clone()));
        }
        Ok(self)
    }
}

/// Stable opaque identity assigned from normalized project path order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModuleId(u32);

impl ModuleId {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkedModuleTarget {
    Internal { id: ModuleId },
    External { package: PackageSpecifier },
    Builtin { name: BuiltinModuleName },
    Missing,
    OutsideProject { path: NormalizedOutsidePath },
    Unsupported { reason: String },
}

/// Errors from local job execution. Parse failures are returned as ordinary
/// per-job results, not through this type.
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

/// Validation failures for raw project inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectInputError {
    InvalidPath(String),
    DuplicateSource(String),
    InvalidTarget(String),
    SourceCountExceeded { limit: usize, attempted: usize },
    SourceBytesExceeded { limit: usize, attempted: usize },
}

/// Failures raised while advancing a project through its authored-resolution
/// and linking phases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectPhaseError {
    InvalidTarget(String),
    UnknownImporter(String),
    DuplicateResolution(ResolutionRequestKey),
    UnknownRequest(ResolutionRequestKey),
    IncompleteLocalAnalysis(Vec<ProjectRelativePath>),
    BudgetExceeded(String),
}

/// Failures raised by the local analysis executor rather than by authored
/// project data or phase validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectExecutionError {
    Local(LocalExecutionError),
}

/// Failure boundary for the staged project API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectError {
    Input(ProjectInputError),
    Phase(ProjectPhaseError),
    Execution(ProjectExecutionError),
}

impl From<ProjectInputError> for ProjectError {
    fn from(error: ProjectInputError) -> Self {
        Self::Input(error)
    }
}

impl From<ProjectPhaseError> for ProjectError {
    fn from(error: ProjectPhaseError) -> Self {
        Self::Phase(error)
    }
}

impl From<ProjectExecutionError> for ProjectError {
    fn from(error: ProjectExecutionError) -> Self {
        Self::Execution(error)
    }
}

impl std::fmt::Display for ProjectInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(path) => write!(f, "invalid project path `{path}`"),
            Self::DuplicateSource(path) => write!(f, "duplicate project source `{path}`"),
            Self::InvalidTarget(path) => write!(f, "invalid resolution target `{path}`"),
            Self::SourceCountExceeded { limit, attempted } => write!(
                f,
                "project source count {attempted} exceeds admission limit {limit}"
            ),
            Self::SourceBytesExceeded { limit, attempted } => write!(
                f,
                "project source bytes {attempted} exceed admission limit {limit}"
            ),
        }
    }
}

impl std::fmt::Display for ProjectPhaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTarget(path) => write!(f, "invalid resolution target `{path}`"),
            Self::UnknownImporter(path) => {
                write!(f, "resolution importer is not a source: `{path}`")
            }
            Self::DuplicateResolution(key) => {
                write!(f, "duplicate resolution for `{}`", key.importer())
            }
            Self::UnknownRequest(key) => write!(
                f,
                "resolution does not match an authored request in `{}`",
                key.importer()
            ),
            Self::IncompleteLocalAnalysis(paths) => write!(
                f,
                "local analysis is incomplete for {} source(s): {}",
                paths.len(),
                paths
                    .iter()
                    .map(ProjectRelativePath::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::BudgetExceeded(message) => write!(f, "project input budget exceeded: {message}"),
        }
    }
}

impl std::fmt::Display for ProjectExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(error) => write!(f, "local analysis execution failed: {error}"),
        }
    }
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(error) => error.fmt(f),
            Self::Phase(error) => error.fmt(f),
            Self::Execution(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ProjectInputError {}
impl std::error::Error for ProjectPhaseError {}
impl std::error::Error for ProjectExecutionError {}
impl std::error::Error for ProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Phase(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_file_reuses_arc_source_allocation() {
        let source: Arc<str> = Arc::from("fetch('/remote');");
        let file = SourceFile::new("main.js", source.clone()).unwrap();

        assert!(std::ptr::eq(source.as_ref(), file.source().as_str()));
    }

    // ── PackageSpecifier ──────────────────────────────────────────

    #[test]
    fn package_specifier_rejects_empty() {
        assert!(PackageSpecifier::new("").is_err());
    }

    #[test]
    fn package_specifier_rejects_whitespace_only() {
        assert!(PackageSpecifier::new("  ").is_err());
        assert!(PackageSpecifier::new("\t").is_err());
    }

    #[test]
    fn package_specifier_strips_surrounding_whitespace() {
        let pkg = PackageSpecifier::new("  lodash  ").unwrap();
        assert_eq!(pkg.as_str(), "lodash");
    }

    #[test]
    fn package_specifier_rejects_interior_whitespace() {
        assert!(PackageSpecifier::new("lodash foo").is_err());
        assert!(PackageSpecifier::new("lodash\tfoo").is_err());
    }

    #[test]
    fn package_specifier_rejects_nul() {
        assert!(PackageSpecifier::new("lod\0ash").is_err());
    }

    #[test]
    fn package_specifier_rejects_relative_syntax() {
        assert!(PackageSpecifier::new("./foo").is_err());
        assert!(PackageSpecifier::new("../foo").is_err());
        assert!(PackageSpecifier::new("/foo").is_err());
        assert!(PackageSpecifier::new("\\foo").is_err());
    }

    #[test]
    fn package_specifier_rejects_non_scoped_with_slash() {
        assert!(PackageSpecifier::new("lodash/fp").is_err());
        assert!(PackageSpecifier::new("a/b/c").is_err());
    }

    #[test]
    fn package_specifier_rejects_bare_at() {
        assert!(PackageSpecifier::new("@").is_err());
    }

    #[test]
    fn package_specifier_rejects_scoped_missing_name() {
        assert!(PackageSpecifier::new("@scope/").is_err());
    }

    #[test]
    fn package_specifier_rejects_scoped_missing_scope() {
        assert!(PackageSpecifier::new("@/name").is_err());
    }

    #[test]
    fn package_specifier_rejects_scoped_double_slash() {
        assert!(PackageSpecifier::new("@scope//name").is_err());
    }

    #[test]
    fn package_specifier_accepts_valid_scoped() {
        let pkg = PackageSpecifier::new("@angular/core").unwrap();
        assert_eq!(pkg.as_str(), "@angular/core");
    }

    #[test]
    fn package_specifier_accepts_valid_bare() {
        let pkg = PackageSpecifier::new("lodash").unwrap();
        assert_eq!(pkg.as_str(), "lodash");
        let pkg = PackageSpecifier::new("express").unwrap();
        assert_eq!(pkg.as_str(), "express");
    }

    #[test]
    fn package_specifier_equality_with_str() {
        let pkg = PackageSpecifier::new("lodash").unwrap();
        assert_eq!(pkg, "lodash");
    }

    // ── BuiltinModuleName ─────────────────────────────────────────

    #[test]
    fn builtin_rejects_empty() {
        assert!(BuiltinModuleName::new("").is_err());
    }

    #[test]
    fn builtin_rejects_whitespace_only() {
        assert!(BuiltinModuleName::new("  ").is_err());
    }

    #[test]
    fn builtin_strips_surrounding_whitespace() {
        let name = BuiltinModuleName::new("  node:fs  ").unwrap();
        assert_eq!(name.as_str(), "node:fs");
    }

    #[test]
    fn builtin_rejects_interior_whitespace() {
        assert!(BuiltinModuleName::new("node: fs").is_err());
        assert!(BuiltinModuleName::new("node:f s").is_err());
        assert!(BuiltinModuleName::new("no de:fs").is_err());
        assert!(BuiltinModuleName::new("node :fs").is_err());
    }

    #[test]
    fn builtin_rejects_nul() {
        assert!(BuiltinModuleName::new("node:f\0s").is_err());
    }

    #[test]
    fn builtin_rejects_missing_prefix() {
        assert!(BuiltinModuleName::new("fs").is_err());
        assert!(BuiltinModuleName::new("nodefs").is_err());
        assert!(BuiltinModuleName::new("Node:fs").is_err());
        assert!(BuiltinModuleName::new("NODE:fs").is_err());
    }

    #[test]
    fn builtin_rejects_empty_name() {
        assert!(BuiltinModuleName::new("node:").is_err());
    }

    #[test]
    fn builtin_accepts_valid_names() {
        let name = BuiltinModuleName::new("node:fs").unwrap();
        assert_eq!(name.as_str(), "node:fs");
        let name = BuiltinModuleName::new("node:path").unwrap();
        assert_eq!(name.as_str(), "node:path");
        let name = BuiltinModuleName::new("node:buffer").unwrap();
        assert_eq!(name.as_str(), "node:buffer");
    }

    #[test]
    fn builtin_equality_with_str() {
        let name = BuiltinModuleName::new("node:fs").unwrap();
        assert_eq!(name, "node:fs");
    }

    // ── NormalizedOutsidePath ─────────────────────────────────────

    #[test]
    fn outside_path_rejects_empty() {
        assert!(NormalizedOutsidePath::new("").is_err());
    }

    #[test]
    fn outside_path_rejects_nul() {
        assert!(NormalizedOutsidePath::new("foo\0bar").is_err());
    }

    #[test]
    fn outside_path_normalizes_backslashes() {
        let path = NormalizedOutsidePath::new("a\\b\\c").unwrap();
        assert_eq!(path.as_str(), "a/b/c");
    }

    #[test]
    fn outside_path_normalizes_dot_segments() {
        let path = NormalizedOutsidePath::new("a/./b").unwrap();
        assert_eq!(path.as_str(), "a/b");
    }

    #[test]
    fn outside_path_normalizes_relative_parent() {
        let path = NormalizedOutsidePath::new("a/b/../c").unwrap();
        assert_eq!(path.as_str(), "a/c");
    }

    #[test]
    fn outside_path_preserves_absolute() {
        let path = NormalizedOutsidePath::new("/a/b").unwrap();
        assert_eq!(path.as_str(), "/a/b");
    }

    #[test]
    fn outside_path_identity_round_trip() {
        let path = NormalizedOutsidePath::new("/usr/lib/node_modules/foo").unwrap();
        assert_eq!(path.as_str(), "/usr/lib/node_modules/foo");
    }
}
