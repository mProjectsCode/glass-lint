//! Obsidian plugin enable/disable rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects rooted plugin-manager calls that change another plugin's enabled
/// state.
pub fn rule() -> Rule {
    Rule::builder("plugins.enable-disable")
        .description("Enables or disables other plugins")
        .category(Category::new("plugins").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_rooted("app.plugins.enablePlugin"))
        .query(QueryDecl::member_call_rooted("app.plugins.disablePlugin"))
        .query(QueryDecl::member_call_rooted(
            "app.plugins.enablePluginAndSave",
        ))
        .query(QueryDecl::member_call_rooted(
            "app.plugins.disablePluginAndSave",
        ))
        .build()
        .unwrap()
}
