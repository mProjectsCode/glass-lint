use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted calls to `app.workspace.getActiveFile`. Provenance follows
/// `this.app`, workspace aliases, static computed properties, source-ordered
/// alias reassignment, and lexical shadowing. Dynamic or unlisted members,
/// local lookalikes, and call arguments are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("workspace.active-file")
        .label("Accesses the active file")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.workspace.getActiveFile"))
        .build()
        .unwrap()
}
