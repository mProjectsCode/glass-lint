mod catalog;
pub mod findings;
mod linter;
pub mod ranges;

pub use catalog::{RuleCatalog, RuleCatalogError};
pub use linter::{LintConfigError, Linter};
pub use ranges::source_range_from_span;
