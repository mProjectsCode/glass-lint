//! Cached Obsidian rule catalog.
//!
//! Catalog construction is provider-owned and cached once.

use std::sync::OnceLock;

use glass_lint_core::rules::Rule;

pub fn obsidian_api_rules() -> &'static [Rule] {
    // Rule construction is immutable after initialization, so all linter
    // profiles share one deterministic provider catalog.
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(crate::rules::all)
}
