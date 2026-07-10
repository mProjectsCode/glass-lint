use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("editor.suggest")
        .label("Registers editor suggestions")
        .category("editor")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("this.registerEditorSuggest"))
        .build()
        .unwrap()
}
