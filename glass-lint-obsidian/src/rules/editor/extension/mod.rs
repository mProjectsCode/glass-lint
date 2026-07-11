use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic member chain `this.registerEditorExtension`.
/// This is a medium-confidence heuristic: it does not prove that `this` is
/// an Obsidian plugin instance and does not follow aliases or reassignment.
/// Static computed names resolving to the configured method are accepted;
/// other receivers, dynamic properties, and near-name methods are excluded.
pub(crate) fn rule() -> Rule {
    Rule::builder("editor.extension")
        .label("Registers editor extensions")
        .category("editor")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerEditorExtension",
        ))
        .build()
        .unwrap()
}
