use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use crate::{ProjectLoadError, error::ProjectOptionError};

const DEFAULT_MAX_FILES: usize = 10_000;
const DEFAULT_MAX_REQUESTS: usize = 50_000;
const DEFAULT_MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_MAX_PROJECT_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_MAX_VISITED_ENTRIES: usize = 250_000;
const DEFAULT_MAX_TIMEOUT_MS: u64 = 5 * 60 * 1000;
const DEFAULT_MAX_CONFIG_COUNT: usize = 100;
const DEFAULT_MAX_CONFIG_DEPTH: usize = 20;
const MAX_TIMEOUT_MS: u64 = 86_400_000;
const DEFAULT_EXTENSIONS: &[&str] = &[".js", ".cjs", ".mjs", ".ts", ".cts", ".mts"];

/// How a filesystem project is selected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectSelection {
    Entry(PathBuf),
    Directory(PathBuf),
    Tsconfig(PathBuf),
}

impl ProjectSelection {
    /// Select one source entry and its reachable internal imports.
    pub fn entry(path: impl Into<PathBuf>) -> Self {
        Self::Entry(path.into())
    }

    /// Select all supported sources under a directory.
    pub fn directory(path: impl Into<PathBuf>) -> Self {
        Self::Directory(path.into())
    }

    /// Select sources through a TypeScript config and its references.
    pub fn tsconfig(path: impl Into<PathBuf>) -> Self {
        Self::Tsconfig(path.into())
    }

    /// Return the path supplied by this selection variant.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Entry(path) | Self::Directory(path) | Self::Tsconfig(path) => path,
        }
    }
}

/// Caller-supplied policy for filesystem project loading.
#[derive(Clone, Debug)]
pub struct ProjectLoadOptions {
    /// Project boundary. If omitted, the selection's directory is used.
    pub(crate) root: Option<PathBuf>,
    /// Maximum number of admitted source files.
    pub(crate) max_files: usize,
    /// Maximum number of resolver requests.
    pub(crate) max_requests: usize,
    /// Maximum bytes accepted for one source.
    pub(crate) max_source_bytes: u64,
    /// Maximum aggregate source bytes reserved before parsing.
    pub(crate) max_project_source_bytes: u64,
    /// Maximum filesystem entries visited during discovery.
    pub(crate) max_visited_entries: usize,
    /// Cooperative total load/link timeout in milliseconds.
    pub(crate) max_timeout_ms: u64,
    /// Maximum number of tsconfig files to process during traversal.
    pub(crate) max_config_count: usize,
    /// Maximum extends or reference chain depth.
    pub(crate) max_config_depth: usize,
    /// Case-insensitive source suffixes accepted by discovery.
    pub(crate) extensions: Vec<String>,
    /// Directory names excluded during discovery and resolution.
    pub(crate) excluded_directories: BTreeSet<String>,
    /// Whether directory traversal and resolution may follow symlinks.
    pub(crate) follow_symlinks: bool,
    /// Resolver extension aliases applied to module requests.
    pub(crate) extension_aliases: BTreeMap<String, Vec<String>>,
}

/// Checked construction for filesystem loading policy.
#[derive(Clone, Debug, Default)]
pub struct ProjectLoadOptionsBuilder {
    options: ProjectLoadOptions,
}

/// A project policy that has passed every cross-field validation rule.
#[derive(Clone, Debug)]
pub struct ValidatedProjectLoadOptions {
    options: ProjectLoadOptions,
    extensions: SourceExtensionSet,
}

/// Normalized, validated source suffixes used by filesystem boundaries.
#[derive(Clone, Debug)]
struct SourceExtensionSet(BTreeSet<String>);

impl SourceExtensionSet {
    fn supports(&self, path: &Path) -> bool {
        let name = path.to_string_lossy().to_ascii_lowercase();
        self.0.iter().any(|extension| name.ends_with(extension))
            && ![".d.ts", ".d.cts", ".d.mts"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
    }
}

impl ProjectLoadOptionsBuilder {
    #[must_use]
    pub fn root(mut self, root: impl Into<PathBuf>) -> Self {
        self.options.root = Some(root.into());
        self
    }

    #[must_use]
    pub fn max_files(mut self, value: usize) -> Self {
        self.options.max_files = value;
        self
    }

    #[must_use]
    pub fn max_requests(mut self, value: usize) -> Self {
        self.options.max_requests = value;
        self
    }

    #[must_use]
    pub fn max_source_bytes(mut self, value: u64) -> Self {
        self.options.max_source_bytes = value;
        self
    }

    #[must_use]
    pub fn max_project_source_bytes(mut self, value: u64) -> Self {
        self.options.max_project_source_bytes = value;
        self
    }

    #[must_use]
    pub fn max_visited_entries(mut self, value: usize) -> Self {
        self.options.max_visited_entries = value;
        self
    }

    #[must_use]
    pub fn max_timeout_ms(mut self, value: u64) -> Self {
        self.options.max_timeout_ms = value;
        self
    }

    #[must_use]
    pub fn max_config_count(mut self, value: usize) -> Self {
        self.options.max_config_count = value;
        self
    }

    #[must_use]
    pub fn max_config_depth(mut self, value: usize) -> Self {
        self.options.max_config_depth = value;
        self
    }

    #[must_use]
    pub fn extensions(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.options.extensions = values.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn excluded_directories(
        mut self,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.options.excluded_directories = values.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn follow_symlinks(mut self, value: bool) -> Self {
        self.options.follow_symlinks = value;
        self
    }

    #[must_use]
    pub fn extension_aliases(mut self, values: BTreeMap<String, Vec<String>>) -> Self {
        self.options.extension_aliases = values;
        self
    }

    pub fn build(self) -> Result<ValidatedProjectLoadOptions, ProjectLoadError> {
        self.options.validated()
    }
}

impl Default for ProjectLoadOptions {
    fn default() -> Self {
        Self {
            root: None,
            max_files: DEFAULT_MAX_FILES,
            max_requests: DEFAULT_MAX_REQUESTS,
            max_source_bytes: DEFAULT_MAX_SOURCE_BYTES,
            max_project_source_bytes: DEFAULT_MAX_PROJECT_SOURCE_BYTES,
            max_visited_entries: DEFAULT_MAX_VISITED_ENTRIES,
            max_timeout_ms: DEFAULT_MAX_TIMEOUT_MS,
            max_config_count: DEFAULT_MAX_CONFIG_COUNT,
            max_config_depth: DEFAULT_MAX_CONFIG_DEPTH,
            extensions: DEFAULT_EXTENSIONS.iter().map(|s| (*s).to_owned()).collect(),
            excluded_directories: [".git", "node_modules", "dist", "build", "target"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            follow_symlinks: false,
            extension_aliases: BTreeMap::new(),
        }
    }
}

impl ProjectLoadOptions {
    /// Validate this policy and mark it safe for the project loader boundary.
    pub(crate) fn validated(self) -> Result<ValidatedProjectLoadOptions, ProjectLoadError> {
        self.validate()?;
        let extensions = SourceExtensionSet(
            self.extensions
                .iter()
                .map(|extension| extension.to_ascii_lowercase())
                .collect(),
        );
        Ok(ValidatedProjectLoadOptions {
            options: self,
            extensions,
        })
    }

    /// Test whether a path under `root` is excluded by any directory name.
    pub fn excludes_path(&self, root: &Path, path: &Path) -> bool {
        path.strip_prefix(root).is_ok_and(|relative| {
            relative.components().any(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .is_some_and(|name| self.excluded_directories.contains(name))
            })
        })
    }

    /// Validate budgets, suffixes, and alias mappings before any I/O begins.
    pub fn validate(&self) -> Result<(), ProjectLoadError> {
        if self.max_files == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::ZeroBudget("max_files"),
            ));
        }
        if self.max_requests == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::ZeroBudget("max_requests"),
            ));
        }
        if self.max_source_bytes == 0
            || self.max_source_bytes > glass_lint_core::MAX_SOURCE_BYTES as u64
        {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::SourceBytesOutOfRange {
                    maximum: glass_lint_core::MAX_SOURCE_BYTES as u64,
                },
            ));
        }
        if self.max_project_source_bytes < self.max_source_bytes {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::ProjectBytesBelowFileBytes,
            ));
        }
        if self.max_visited_entries == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::ZeroBudget("max_visited_entries"),
            ));
        }
        if self.max_timeout_ms == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::ZeroBudget("max_timeout_ms"),
            ));
        }
        if self.max_timeout_ms > MAX_TIMEOUT_MS {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::TimeoutOutOfRange {
                    maximum: MAX_TIMEOUT_MS,
                },
            ));
        }
        if self.max_config_count == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::ZeroBudget("max_config_count"),
            ));
        }
        if self.max_config_depth == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::ZeroBudget("max_config_depth"),
            ));
        }
        if self.extensions.is_empty()
            || self
                .extensions
                .iter()
                .any(|extension| !Self::valid_extension(extension))
        {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::InvalidExtensions,
            ));
        }
        if self.extension_aliases.iter().any(|(extension, aliases)| {
            !Self::valid_extension(extension)
                || aliases.is_empty()
                || aliases.iter().any(|alias| !Self::valid_extension(alias))
        }) {
            return Err(ProjectLoadError::InvalidOptions(
                ProjectOptionError::InvalidExtensionAliases,
            ));
        }
        Ok(())
    }
}

impl Default for ValidatedProjectLoadOptions {
    fn default() -> Self {
        ProjectLoadOptions::default()
            .validated()
            .expect("built-in default options are valid")
    }
}

impl ValidatedProjectLoadOptions {
    /// Start building a project-loading policy with validated defaults.
    pub fn builder() -> ProjectLoadOptionsBuilder {
        ProjectLoadOptionsBuilder::default()
    }

    pub fn root(&self) -> Option<&Path> {
        self.options.root.as_deref()
    }

    pub fn max_files(&self) -> usize {
        self.options.max_files
    }

    pub fn max_requests(&self) -> usize {
        self.options.max_requests
    }

    pub fn max_source_bytes(&self) -> u64 {
        self.options.max_source_bytes
    }

    pub fn max_project_source_bytes(&self) -> u64 {
        self.options.max_project_source_bytes
    }

    pub fn max_visited_entries(&self) -> usize {
        self.options.max_visited_entries
    }

    pub fn max_timeout_ms(&self) -> u64 {
        self.options.max_timeout_ms
    }

    pub fn max_config_count(&self) -> usize {
        self.options.max_config_count
    }

    pub fn max_config_depth(&self) -> usize {
        self.options.max_config_depth
    }

    pub fn extensions(&self) -> impl Iterator<Item = &str> {
        self.extensions.0.iter().map(String::as_str)
    }

    pub fn extension_aliases(&self) -> &BTreeMap<String, Vec<String>> {
        &self.options.extension_aliases
    }

    pub fn follow_symlinks(&self) -> bool {
        self.options.follow_symlinks
    }

    pub fn supports(&self, path: &Path) -> bool {
        self.extensions.supports(path)
    }

    pub fn excludes_path(&self, root: &Path, path: &Path) -> bool {
        self.options.excludes_path(root, path)
    }
}

impl ProjectLoadOptions {
    fn valid_extension(extension: &str) -> bool {
        extension.len() >= 2
            && extension.starts_with('.')
            && !extension
                .chars()
                .any(|character| character == '/' || character == '\\' || character == '\0')
    }
}
