//! Rule catalog selection, semantic execution, and finding assembly.
//!
//! Linting parses/analyzes once, projects selected matcher plans, then converts
//! located evidence into deterministic findings. Range policy and catalog
//! policy stay separate from semantic fact construction.

mod batch;
mod catalog;
mod linter;
pub mod report;
mod selection;

pub use batch::{BatchOptions, BatchResult, BatchResults, BatchStartError};
pub use catalog::{ProviderCatalogError, RuleCatalog, RuleCompilationError};
pub use linter::{Linter, LinterConfig};
pub use report::{ProjectAnalysis, ProjectAnalysisTimings};
pub use selection::{
    LintConfigError, PreparedRuleSelection, RuleBaseline, RuleOverride, RuleSelection, RuleState,
};
