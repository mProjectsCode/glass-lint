//! Obsidian app-scoped storage rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects rooted app-scoped storage and secret-store operations. Rooted
/// aliases and static computed properties retain provenance; local lookalikes,
/// shadowed app bindings, dynamic properties, and unrelated storage objects do
/// not match.
pub fn rule() -> Rule {
    Rule::builder("storage.app-data")
        .description("Reads or writes app-scoped storage and secrets")
        .category(Category::new("storage").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_rooted("app.loadLocalStorage"))
        .query(QueryDecl::member_call_rooted("app.saveLocalStorage"))
        .query(QueryDecl::member_call_rooted("app.secretStorage.getSecret"))
        .query(QueryDecl::member_call_rooted("app.secretStorage.setSecret"))
        .query(QueryDecl::member_call_rooted("app.secretStorage.listSecrets"))
        .build()
        .unwrap()
}
