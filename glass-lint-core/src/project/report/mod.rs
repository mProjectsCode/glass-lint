//! Project finding assembly and deterministic evidence ownership.

use crate::{
    ProjectRelativePath,
    project::{AnalysisReport, Evidence, Finding, ReportCompletion},
};

/// Why independently produced reports could not be combined losslessly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReportCombineError {
    /// At least one report is required to define schema and tool identity.
    Empty,
    /// Every report in one aggregate must use the same schema contract.
    SchemaMismatch { expected: u32, actual: u32 },
    /// Reports from different tool versions are not silently mixed.
    ToolVersionMismatch { expected: String, actual: String },
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
        }
    }
}

impl std::error::Error for ReportCombineError {}

impl AnalysisReport {
    /// Losslessly combine reports produced by independent analyses.
    ///
    /// ```
    /// # use glass_lint_core::{Environment, Linter, LinterConfig, RuleCatalog, AnalysisReport};
    /// let linter = Linter::new(LinterConfig::new(
    ///     vec![RuleCatalog::new("example", vec![]).unwrap()],
    ///     Environment::default(),
    /// ))
    /// .unwrap();
    /// let first = linter.lint_snippet("", "first.js").unwrap();
    /// let second = linter.lint_snippet("", "second.js").unwrap();
    /// let combined = AnalysisReport::combine([first, second]).unwrap();
    /// assert_eq!(combined.files.len(), 2);
    /// ```
    pub fn combine(reports: impl IntoIterator<Item = Self>) -> Result<Self, ReportCombineError> {
        let mut reports = reports.into_iter();
        let Some(mut combined) = reports.next() else {
            return Err(ReportCombineError::Empty);
        };
        for mut report in reports {
            if report.schema_version != combined.schema_version {
                return Err(ReportCombineError::SchemaMismatch {
                    expected: combined.schema_version,
                    actual: report.schema_version,
                });
            }
            if report.tool_version != combined.tool_version {
                return Err(ReportCombineError::ToolVersionMismatch {
                    expected: combined.tool_version,
                    actual: report.tool_version,
                });
            }
            combined.files.append(&mut report.files);
            combined.diagnostics.append(&mut report.diagnostics);
            combined.operations += report.operations;
            if report.completion == ReportCompletion::Partial {
                combined.completion = ReportCompletion::Partial;
            }
        }
        combined
            .files
            .sort_by(|left, right| left.path.cmp(&right.path));
        combined.diagnostics.sort_by(|left, right| {
            (
                left.path().map(ProjectRelativePath::as_str),
                left.code(),
                left.message(),
            )
                .cmp(&(
                    right.path().map(ProjectRelativePath::as_str),
                    right.code(),
                    right.message(),
                ))
        });
        Ok(combined)
    }
}

impl Finding {
    /// Attach a shared evidence slice. Any previously set shared slice is
    /// replaced. Local evidence is preserved.
    pub fn set_shared_evidence(&mut self, shared: std::sync::Arc<[Evidence]>) {
        self.evidence.set_shared(shared);
    }
}

#[cfg(test)]
mod tests;
