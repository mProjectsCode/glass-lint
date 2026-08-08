//! Project finding assembly and deterministic evidence ownership.

use std::collections::BTreeSet;

use crate::project::AnalysisReport;

/// Why independently produced reports could not be combined losslessly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReportCombineError {
    /// At least one report is required to define schema and tool identity.
    Empty,
    /// Every report in one aggregate must use the same schema contract.
    SchemaMismatch { expected: u32, actual: u32 },
    /// Reports from different tool versions are not silently mixed.
    ToolVersionMismatch { expected: String, actual: String },
    /// Two input reports contain the same normalized file path.
    DuplicateFilePath {
        path: crate::project::ProjectRelativePath,
    },
}

impl std::fmt::Display for ReportCombineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("cannot combine an empty report collection"),
            Self::SchemaMismatch { expected, actual } => write!(
                formatter,
                "report schema mismatch: expected {expected}, found {actual}"
            ),
            Self::ToolVersionMismatch { expected, actual } => write!(
                formatter,
                "report tool version mismatch: expected {expected}, found {actual}"
            ),
            Self::DuplicateFilePath { path } => {
                write!(formatter, "duplicate report file path: {path}")
            }
        }
    }
}

impl std::error::Error for ReportCombineError {}

impl AnalysisReport {
    /// Losslessly combine reports produced by independent analyses.
    ///
    /// ```
    /// # use glass_lint_core::{Environment, Linter, LinterConfig, RuleCatalog, project::AnalysisReport};
    /// let linter = Linter::new(LinterConfig::new(
    ///     vec![RuleCatalog::new("example", vec![]).unwrap()],
    ///     Environment::default(),
    /// ))
    /// .unwrap();
    /// let first = linter.lint_source(glass_lint_core::project::SourceFile::new("first.js", "").unwrap()).unwrap();
    /// let second = linter.lint_source(glass_lint_core::project::SourceFile::new("second.js", "").unwrap()).unwrap();
    /// let combined = AnalysisReport::combine([first, second]).unwrap();
    /// assert_eq!(combined.files().len(), 2);
    /// ```
    pub fn combine(reports: impl IntoIterator<Item = Self>) -> Result<Self, ReportCombineError> {
        let reports: Vec<Self> = reports.into_iter().collect();
        let Some(first) = reports.first() else {
            return Err(ReportCombineError::Empty);
        };
        let mut paths = BTreeSet::new();
        for report in &reports {
            if report.schema_version() != first.schema_version() {
                return Err(ReportCombineError::SchemaMismatch {
                    expected: first.schema_version(),
                    actual: report.schema_version(),
                });
            }
            if report.tool_version() != first.tool_version() {
                return Err(ReportCombineError::ToolVersionMismatch {
                    expected: first.tool_version().into(),
                    actual: report.tool_version().into(),
                });
            }
            for file in report.files() {
                if !paths.insert(file.path().clone()) {
                    return Err(ReportCombineError::DuplicateFilePath {
                        path: file.path().clone(),
                    });
                }
            }
        }

        let mut reports = reports.into_iter();
        let Some(mut combined) = reports.next() else {
            return Err(ReportCombineError::Empty);
        };
        for report in reports {
            combined = combined.merge(report);
        }
        Ok(combined.finalize())
    }
}

#[cfg(test)]
mod tests;
