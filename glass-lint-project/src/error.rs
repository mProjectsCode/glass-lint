use std::{fmt, path::PathBuf};

use glass_lint_core::ProjectInputError;

/// Operational and semantic errors from project construction.
#[derive(Debug)]
pub enum ProjectLoadError {
    /// Loader options violate a configured invariant.
    InvalidOptions(String),
    /// The selected path does not exist.
    SelectionNotFound(PathBuf),
    /// An entry or config selection is not a file.
    SelectionNotFile(PathBuf),
    /// The selection escapes the configured project root.
    SelectionOutsideRoot { selection: PathBuf, root: PathBuf },
    /// A file extension is not enabled for loading.
    UnsupportedSource(PathBuf),
    /// Filesystem traversal or reading failed.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The file-count budget was exceeded.
    TooManyFiles(usize),
    /// The resolver-request budget was exceeded.
    TooManyRequests(usize),
    /// A source exceeded the configured byte budget.
    SourceTooLarge {
        path: PathBuf,
        bytes: u64,
        limit: u64,
    },
    /// Core rejected normalized project input.
    Core(ProjectInputError),
}

impl fmt::Display for ProjectLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions(message) => write!(f, "invalid project options: {message}"),
            Self::SelectionNotFound(path) => {
                write!(f, "project selection does not exist: {}", path.display())
            }
            Self::SelectionNotFile(path) => {
                write!(f, "project entry is not a file: {}", path.display())
            }
            Self::SelectionOutsideRoot { selection, root } => write!(
                f,
                "project selection {} is outside project root {}",
                selection.display(),
                root.display()
            ),
            Self::UnsupportedSource(path) => {
                write!(f, "unsupported project source: {}", path.display())
            }
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::TooManyFiles(limit) => write!(f, "project file limit exceeded ({limit})"),
            Self::TooManyRequests(limit) => {
                write!(f, "project resolution request limit exceeded ({limit})")
            }
            Self::SourceTooLarge { path, bytes, limit } => {
                write!(f, "{} is {bytes} bytes, exceeding {limit}", path.display())
            }
            Self::Core(error) => write!(f, "core project error: {error}"),
        }
    }
}

impl std::error::Error for ProjectLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Core(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ProjectInputError> for ProjectLoadError {
    fn from(error: ProjectInputError) -> Self {
        Self::Core(error)
    }
}
