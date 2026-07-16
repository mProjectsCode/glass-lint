//! Cached Obsidian rule catalog and disclosure lookup.
//!
//! Catalog construction is provider-owned and cached once, while disclosure
//! policy remains a separate mapping from unprefixed rule IDs to UI categories.

use std::sync::OnceLock;

use glass_lint_core::rules::Rule;

mod disclosures;

pub fn obsidian_api_rules() -> &'static [Rule] {
    // Rule construction is immutable after initialization, so all linter
    // profiles share one deterministic provider catalog.
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(crate::rules::all)
}

pub fn disclosures_for_rule(id: &str) -> &'static [&'static str] {
    disclosures::for_rule(id)
}
