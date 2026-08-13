//! Obsidian editor-extension registration rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects the syntactic member chain `this.registerEditorExtension`.
/// The instance matcher requires a proven Obsidian `Plugin` receiver and
/// accepts static computed names and proven aliases resolving to the configured
/// method; dynamic properties, reassignment, and near-name methods are
/// excluded.
pub fn rule() -> Rule {
    Rule::catalog_builder("editor.extension")
        .description("Registers editor extensions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian",
            "Plugin",
            "registerEditorExtension",
        ))
        .build()
        .unwrap()
}
