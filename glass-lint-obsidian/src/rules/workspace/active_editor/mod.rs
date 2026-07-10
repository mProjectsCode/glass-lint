use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("workspace.active-editor")
        .label("Accesses the active editor")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_read("app.workspace.activeEditor"))
        .build()
        .unwrap()
}
