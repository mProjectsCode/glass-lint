use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
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
