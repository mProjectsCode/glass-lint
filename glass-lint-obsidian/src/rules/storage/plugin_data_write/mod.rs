//! Obsidian plugin-data write rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `this.saveData()` plugin-storage call, including a
/// statically computed `saveData` property. The instance matcher requires a
/// proven Obsidian `Plugin` receiver and does not follow aliases, shadowing, or
/// reassignment; dynamic properties and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("storage.plugin-data-write")
        .description("Writes plugin data")
        .category("storage")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian", "Plugin", "saveData",
        ))
        .build()
        .unwrap()
}
