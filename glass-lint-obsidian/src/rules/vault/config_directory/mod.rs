//! Obsidian configuration-directory indicator rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects string and static-template fragments containing the exact
/// `.obsidian/` or `.obsidian\\` configuration-directory markers. This is a
/// medium-confidence literal heuristic: it does not establish vault or path
/// provenance, reconstruct dynamic values, or normalize casing and separators
/// beyond the two configured spellings. Bounded constant compositions are
/// evaluated by core.
pub fn rule() -> Rule {
    Rule::catalog_builder("vault.config-directory")
        .description("References .obsidian configuration paths")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(EventQuery::string_contains(".obsidian/"))
        .query(EventQuery::string_contains(".obsidian\\"))
        .query(EventQuery::member_read_rooted("app.vault.configDir"))
        .build()
        .unwrap()
}
