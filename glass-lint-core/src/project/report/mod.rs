//! Project finding assembly and deterministic evidence ownership.

use crate::project::{AnalysisReport, ProjectRelativePath, ReportCompletion};

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
    /// # use glass_lint_core::{Environment, Linter, LinterConfig, RuleCatalog, project::AnalysisReport};
    /// let linter = Linter::new(LinterConfig::new(
    ///     vec![RuleCatalog::new("example", vec![]).unwrap()],
    ///     Environment::default(),
    /// ))
    /// .unwrap();
    /// let first = linter.lint_snippet("", "first.js").unwrap();
    /// let second = linter.lint_snippet("", "second.js").unwrap();
    /// let combined = AnalysisReport::combine([first, second]).unwrap();
    /// assert_eq!(combined.files().len(), 2);
    /// ```
    pub fn combine(reports: impl IntoIterator<Item = Self>) -> Result<Self, ReportCombineError> {
        let mut reports = reports.into_iter();
        let Some(first) = reports.next() else {
            return Err(ReportCombineError::Empty);
        };
        let (
            schema_version,
            tool_version,
            mut files,
            mut diagnostics,
            mut operations,
            mut completion,
        ) = first.into_parts();
        for report in reports {
            let (r_schema, r_tool, r_files, r_diags, r_ops, r_comp) = report.into_parts();
            if r_schema != schema_version {
                return Err(ReportCombineError::SchemaMismatch {
                    expected: schema_version,
                    actual: r_schema,
                });
            }
            if r_tool != tool_version {
                return Err(ReportCombineError::ToolVersionMismatch {
                    expected: tool_version,
                    actual: r_tool,
                });
            }
            files.extend(r_files);
            diagnostics.extend(r_diags);
            operations += r_ops;
            if r_comp == ReportCompletion::Partial {
                completion = ReportCompletion::Partial;
            }
        }
        files.sort_by(|left, right| left.path().cmp(right.path()));
        diagnostics.sort_by(|left, right| {
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
        Ok(Self::new(
            schema_version,
            tool_version,
            files,
            diagnostics,
            operations,
            completion,
        ))
    }
}

#[cfg(test)]
mod tests;
