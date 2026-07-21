//! Public project input, resolution, and report contracts.
//!
//! These types make project analysis filesystem-free: callers provide authored
//! sources and explicit resolver outcomes, and reports retain normalized paths
//! and source ranges for deterministic downstream rendering.

use std::{borrow::Borrow, ops::Deref, path::Path};

mod input;
mod report;

pub use input::{
    LinkedModuleTarget, ModuleId, ProjectInput, ProjectInputError, ResolutionRequest,
    ResolutionRequestKey, ResolutionRequestKind, ResolverOutcome, SourceFile,
};
pub use report::{
    AnalysisDiagnostic, AnalysisOperationCounts, AnalysisReport, AnalysisReportSummary, Diagnostic,
    DiagnosticCode, DiagnosticKind, Evidence, FileReport, Finding, ReportCompletion,
    SourceLocation,
};

/// Whether a module request uses syntax that denotes an authored/internal
/// target.
pub fn is_internal_module_request(request: &str) -> bool {
    request.starts_with('.') || request.starts_with('/') || request.starts_with('#')
}

/// A normalized project-relative path whose representation cannot be mutated
/// back into an absolute or escaping path by callers.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct ProjectRelativePath(String);

impl<'de> serde::Deserialize<'de> for ProjectRelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::new(&raw).map_err(|error| serde::de::Error::custom(error.to_string()))
    }
}

impl ProjectRelativePath {
    /// Validate and normalize a project-relative path.
    pub fn new(path: impl AsRef<str>) -> Result<Self, ProjectInputError> {
        crate::project::input::normalize_relative(path)
    }

    pub(crate) fn from_normalized(path: String) -> Self {
        Self(path)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for ProjectRelativePath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Deref for ProjectRelativePath {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ProjectRelativePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for ProjectRelativePath {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl std::fmt::Display for ProjectRelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<&str> for ProjectRelativePath {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}
