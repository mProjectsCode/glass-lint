//! Obsidian app-scoped storage rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects rooted app-scoped storage and secret-store operations. Rooted
/// aliases and static computed properties retain provenance; local lookalikes,
/// shadowed app bindings, dynamic properties, and unrelated storage objects do
/// not match.
pub fn rule() -> Rule {
    Rule::builder("storage.app-data")
        .description("Reads or writes app-scoped storage and secrets")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted("app.loadLocalStorage"))
        .query(EventQuery::member_call_rooted("app.saveLocalStorage"))
        .query(EventQuery::member_call_rooted(
            "app.secretStorage.getSecret",
        ))
        .query(EventQuery::member_call_rooted(
            "app.secretStorage.setSecret",
        ))
        .query(EventQuery::member_call_rooted(
            "app.secretStorage.listSecrets",
        ))
        .build()
        .unwrap()
}
