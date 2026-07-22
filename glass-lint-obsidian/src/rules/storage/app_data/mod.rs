//! Obsidian app-scoped storage rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(MatcherDecl::rooted_member_call("app.loadLocalStorage"))
        .declaration(MatcherDecl::rooted_member_call("app.saveLocalStorage"))
        .declaration(MatcherDecl::rooted_member_call(
            "app.secretStorage.getSecret",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.secretStorage.setSecret",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.secretStorage.listSecrets",
        ))
        .build()
        .unwrap()
}
