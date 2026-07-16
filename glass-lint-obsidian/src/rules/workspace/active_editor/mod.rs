use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted reads of `app.workspace.activeEditor`. Provenance follows
/// `this.app`, workspace aliases, static computed properties, source-ordered
/// alias reassignment, and lexical shadowing. Dynamic or unlisted members,
/// local lookalikes, and the read value itself are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("workspace.active-editor")
        .label("Accesses the active editor")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_read("app.workspace.activeEditor"))
        .build()
        .unwrap()
}
