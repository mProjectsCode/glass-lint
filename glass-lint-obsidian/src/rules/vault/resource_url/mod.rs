//! Obsidian vault resource-URL rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects rooted calls to vault and adapter `getResourcePath`, plus literal
/// or static-template fragments containing `obsidian://`. Rooted provenance
/// follows `this.app`, direct receiver aliases, static computed properties,
/// source-ordered reassignment, and lexical shadowing; the URL marker remains
/// a raw literal heuristic. Arguments, dynamic string reconstruction, and
/// other URL schemes are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.resource-url")
        .description("Accesses attachment resource paths")
        .category(Category::new("vault").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted("app.vault.getResourcePath"))
        .query(EventQuery::member_call_rooted(
            "app.vault.adapter.getResourcePath",
        ))
        .query(EventQuery::string_contains("obsidian://"))
        .build()
        .unwrap()
}
