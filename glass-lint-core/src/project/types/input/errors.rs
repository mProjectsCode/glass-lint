use super::{ProjectRelativePath, ResolutionRequestKey};

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
