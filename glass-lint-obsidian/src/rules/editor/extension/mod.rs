//! Obsidian editor-extension registration rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic member chain `this.registerEditorExtension`.
/// The instance matcher requires a proven Obsidian `Plugin` receiver and
/// accepts static computed names resolving to the configured method; dynamic
/// properties, aliases, reassignment, and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("editor.extension")
        .description("Registers editor extensions")
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
