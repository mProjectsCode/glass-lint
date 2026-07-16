use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted calls to `app.workspace.openLinkText` and
/// `app.workspace.getLeaf.openFile`. Provenance follows `this.app`, workspace
/// aliases, static computed properties, source-ordered alias reassignment,
/// and lexical shadowing. Dynamic or unlisted members, local lookalikes, and
/// call arguments are not analyzed. Returned leaves are followed through
/// simple aliases while reassignment and shadowing remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("workspace.open")
        .label("Opens files through the workspace")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.workspace.openLinkText"))
        .matcher(Matcher::rooted_member_call(
            "app.workspace.getLeaf.openFile",
        ))
        .matcher(Matcher::returned_member_call(
            "app.workspace.getLeaf",
            "openFile",
        ))
        .build()
        .unwrap()
}
