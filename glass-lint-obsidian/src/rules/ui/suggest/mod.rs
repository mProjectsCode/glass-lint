use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `this.registerEditorSuggest()` registration call,
/// including a static computed property. It does not prove an Obsidian
/// receiver or follow aliases/reassignment; other receivers, dynamic
/// properties, and near-name methods are excluded.
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.suggest")
        .label("Registers editor suggestions")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("this.registerEditorSuggest"))
        .build()
        .unwrap()
}
