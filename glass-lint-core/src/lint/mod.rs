//! Rule catalog selection, semantic execution, and finding assembly.
//!
//! Linting parses/analyzes once, projects selected matcher plans, then converts
//! located evidence into deterministic findings. Range policy and catalog
//! policy stay separate from semantic fact construction.

mod catalog;
pub mod findings;
mod linter;
pub mod ranges;

pub use catalog::{RuleCatalog, RuleCatalogError};
pub use linter::{LintConfigError, Linter};
pub use ranges::source_range_from_span;
