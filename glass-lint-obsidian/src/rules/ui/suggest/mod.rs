use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `this.registerEditorSuggest()` registration call,
/// including a static computed property. It does not prove an Obsidian
/// receiver or follow aliases/reassignment; other receivers, dynamic
/// properties, and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("ui.suggest")
        .label("Registers editor suggestions")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerEditorSuggest",
        ))
        .build()
        .unwrap()
}
