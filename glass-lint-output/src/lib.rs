//! Terminal-oriented report presentation for Glass Lint front ends.
//!
//! Construct [`PrettyFile`] values from core file reports and their source
//! text, then pass them to [`PrettyReports`] for deterministic grouped output.

pub use glass_lint_core::{RuleId, Severity};

mod report;

pub use report::{PrettyFile, PrettyOptions, PrettyReport, PrettyReports, visible_text};

#[cfg(test)]
mod tests;
