//! Rule catalog selection, semantic execution, and finding assembly.
//!
//! Linting parses/analyzes once, projects selected matcher plans, then converts
//! located evidence into deterministic findings. Range policy and catalog
//! policy stay separate from semantic fact construction.

mod catalog;
mod linter;
mod ranges;
mod report;
mod selection;

pub use catalog::{ProviderCatalogError, RuleCatalog};
pub use linter::{Linter, LinterConfig};
pub use report::{ProjectAnalysis, ReportAssembly};
pub use selection::{LintConfigError, RuleBaseline, RuleOverride, RuleSelection, RuleState};
