//! Obsidian active-editor workspace rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects rooted reads of `app.workspace.activeEditor`. Provenance follows
/// `this.app`, workspace aliases, static computed properties, source-ordered
/// alias reassignment, and lexical shadowing. Dynamic or unlisted members,
/// local lookalikes, and the read value itself are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("workspace.active-editor")
        .description("Accesses the active editor")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_read_rooted("app.workspace.activeEditor"))
        .build()
        .unwrap()
}
