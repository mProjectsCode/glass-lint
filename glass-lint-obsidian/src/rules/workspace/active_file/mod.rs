//! Obsidian active-file workspace rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to `app.workspace.getActiveFile`. Provenance follows
/// `this.app`, workspace aliases, static computed properties, source-ordered
/// alias reassignment, and lexical shadowing. Dynamic or unlisted members,
/// local lookalikes, and call arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("workspace.active-file")
        .description("Accesses the active file")
        .category(Category::new("workspace").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.getActiveFile")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
