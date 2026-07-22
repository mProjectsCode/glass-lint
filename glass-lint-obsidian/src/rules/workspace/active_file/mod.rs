//! Obsidian active-file workspace rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to `app.workspace.getActiveFile`. Provenance follows
/// `this.app`, workspace aliases, static computed properties, source-ordered
/// alias reassignment, and lexical shadowing. Dynamic or unlisted members,
/// local lookalikes, and call arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("workspace.active-file")
        .description("Accesses the active file")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.getActiveFile",
        ))
        .build()
        .unwrap()
}
