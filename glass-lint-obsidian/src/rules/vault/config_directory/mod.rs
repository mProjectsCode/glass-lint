//! Obsidian configuration-directory indicator rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects string and static-template fragments containing the exact
/// `.obsidian/` or `.obsidian\\` configuration-directory markers. This is a
/// medium-confidence literal heuristic: it does not establish vault or path
/// provenance, reconstruct dynamic values or concatenations, or normalize
/// casing and separators beyond the two configured spellings.
pub fn rule() -> Rule {
    Rule::builder("vault.config-directory")
        .description("References .obsidian configuration paths")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::string_contains(".obsidian/"))
        .matcher(Matcher::string_contains(".obsidian\\"))
        .matcher(Matcher::rooted_member_read("app.vault.configDir"))
        .build()
        .unwrap()
}
