//! Obsidian plugin-data write rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects the syntactic `this.saveData()` plugin-storage call, including a
/// statically computed `saveData` property. The instance matcher requires a
/// proven Obsidian `Plugin` receiver and does not follow aliases, shadowing, or
/// reassignment; dynamic properties and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("storage.plugin-data-write")
        .description("Writes plugin data")
        .category(Category::new("storage").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian", "Plugin", "saveData",
        ))
        .build()
        .unwrap()
}
