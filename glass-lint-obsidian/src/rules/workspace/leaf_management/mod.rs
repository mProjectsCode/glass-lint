use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted calls to `getLeavesOfType`, `detachLeavesOfType`, and
/// `revealLeaf` on `app.workspace`. Provenance follows `this.app`, workspace
/// aliases, static computed properties, source-ordered alias reassignment,
/// and lexical shadowing. Dynamic or unlisted members, local lookalikes, and
/// call arguments are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("workspace.leaf-management")
        .label("Manages workspace leaves")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.workspace.getLeavesOfType"))
        .matcher(Matcher::rooted_member_call(
            "app.workspace.detachLeavesOfType",
        ))
        .matcher(Matcher::rooted_member_call("app.workspace.revealLeaf"))
        .build()
        .unwrap()
}
