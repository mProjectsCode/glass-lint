use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("editor.extension")
        .label("Registers editor extensions")
        .category("editor")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call(
            "this.registerEditorExtension",
        ))
        .build()
        .unwrap()
}
