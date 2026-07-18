//! Obsidian app-scoped storage rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted app-scoped storage and secret-store operations. Rooted
/// aliases and static computed properties retain provenance; local lookalikes,
/// shadowed app bindings, dynamic properties, and unrelated storage objects do
/// not match.
pub fn rule() -> Rule {
    Rule::builder("storage.app-data")
        .description("Reads or writes app-scoped storage and secrets")
        .category("storage")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.loadLocalStorage"))
        .matcher(Matcher::rooted_member_call("app.saveLocalStorage"))
        .matcher(Matcher::rooted_member_call("app.secretStorage.getSecret"))
        .matcher(Matcher::rooted_member_call("app.secretStorage.setSecret"))
        .matcher(Matcher::rooted_member_call("app.secretStorage.listSecrets"))
        .build()
        .unwrap()
}
