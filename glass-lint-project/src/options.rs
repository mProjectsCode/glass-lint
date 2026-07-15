use crate::ProjectLoadError;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const DEFAULT_MAX_FILES: usize = 10_000;
const DEFAULT_MAX_REQUESTS: usize = 50_000;
const DEFAULT_MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_EXTENSIONS: &[&str] = &[".js", ".cjs", ".mjs", ".ts", ".cts", ".mts"];

/// How a filesystem project is selected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectSelection {
    Entry(PathBuf),
    Directory(PathBuf),
    TsConfig(PathBuf),
}

impl ProjectSelection {
    pub fn entry(path: impl Into<PathBuf>) -> Self {
        Self::Entry(path.into())
    }

    pub fn directory(path: impl Into<PathBuf>) -> Self {
        Self::Directory(path.into())
    }

    pub fn tsconfig(path: impl Into<PathBuf>) -> Self {
        Self::TsConfig(path.into())
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        match self {
            Self::Entry(path) | Self::Directory(path) | Self::TsConfig(path) => path,
        }
    }
}

/// Validated policy for filesystem project loading.
#[derive(Clone, Debug)]
pub struct ProjectLoadOptions {
    /// Project boundary. If omitted, the selection's directory is used.
    pub root: Option<PathBuf>,
    pub max_files: usize,
    pub max_requests: usize,
    pub max_source_bytes: u64,
    pub extensions: Vec<String>,
    pub excluded_directories: BTreeSet<String>,
    pub follow_symlinks: bool,
    pub extension_aliases: BTreeMap<String, Vec<String>>,
}

impl Default for ProjectLoadOptions {
    fn default() -> Self {
        Self {
            root: None,
            max_files: DEFAULT_MAX_FILES,
            max_requests: DEFAULT_MAX_REQUESTS,
            max_source_bytes: DEFAULT_MAX_SOURCE_BYTES,
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
    pub fn validate(&self) -> Result<(), ProjectLoadError> {
        if self.max_files == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                "max_files must be positive".into(),
            ));
        }
        if self.max_requests == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                "max_requests must be positive".into(),
            ));
        }
        if self.max_source_bytes == 0
            || self.max_source_bytes > glass_lint_core::MAX_SOURCE_BYTES as u64
        {
            return Err(ProjectLoadError::InvalidOptions(format!(
                "max_source_bytes must be between 1 and {}",
                glass_lint_core::MAX_SOURCE_BYTES
            )));
        }
        if self.extensions.is_empty()
            || self
                .extensions
                .iter()
                .any(|extension| !valid_extension(extension))
        {
            return Err(ProjectLoadError::InvalidOptions(
                "extensions must be non-empty file suffixes".into(),
            ));
        }
        if self.extension_aliases.iter().any(|(extension, aliases)| {
            !valid_extension(extension)
                || aliases.is_empty()
                || aliases.iter().any(|alias| !valid_extension(alias))
        }) {
            return Err(ProjectLoadError::InvalidOptions(
                "extension aliases must map file suffixes to non-empty suffix lists".into(),
            ));
        }
        Ok(())
    }
}

fn valid_extension(extension: &str) -> bool {
    extension.len() >= 2
        && extension.starts_with('.')
        && !extension
            .chars()
            .any(|character| character == '/' || character == '\\' || character == '\0')
}
