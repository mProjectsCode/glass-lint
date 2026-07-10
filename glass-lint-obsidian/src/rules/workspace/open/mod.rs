use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("workspace.open")
        .label("Opens files through the workspace")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.workspace.openLinkText"))
        .matcher(Matcher::rooted_member_call(
            "app.workspace.getLeaf.openFile",
        ))
        .build()
        .unwrap()
}
