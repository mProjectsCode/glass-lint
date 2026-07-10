use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("workspace.layout")
        .label("Reads or writes workspace layout")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::rooted_member_call("app.workspace.getLayout"))
        .matcher(Matcher::rooted_member_call("app.workspace.changeLayout"))
        .matcher(Matcher::rooted_member_call(
            "app.workspace.requestSaveLayout",
        ))
        .build()
        .unwrap()
}
