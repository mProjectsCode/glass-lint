//! Obsidian plugin enable/disable rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects rooted plugin-manager calls that change
/// another plugin's enabled state.
pub fn rule() -> Rule {
    Rule::catalog_builder("plugins.enable-disable")
        .description("Enables or disables other plugins")
        .severity(Severity::Warning)
        .confidence(Confidence::Low)
        .query(EventQuery::member_call_rooted("app.plugins.enablePlugin"))
        .query(EventQuery::member_call_rooted("app.plugins.disablePlugin"))
        .query(EventQuery::member_call_rooted(
            "app.plugins.enablePluginAndSave",
        ))
        .query(EventQuery::member_call_rooted(
            "app.plugins.disablePluginAndSave",
        ))
        .build()
        .unwrap()
}
