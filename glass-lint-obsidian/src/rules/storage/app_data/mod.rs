//! Obsidian app-scoped storage rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.loadLocalStorage")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.saveLocalStorage")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.secretStorage.getSecret")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.secretStorage.setSecret")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.secretStorage.listSecrets")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
