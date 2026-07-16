//! Obsidian plugin enable/disable rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted plugin-manager calls that change another plugin's enabled
/// state.
pub fn rule() -> Rule {
    Rule::builder("plugins.enable-disable")
        .label("Enables or disables other plugins")
        .category("plugins")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.plugins.enablePlugin"))
        .matcher(Matcher::rooted_member_call("app.plugins.disablePlugin"))
        .matcher(Matcher::rooted_member_call(
            "app.plugins.enablePluginAndSave",
        ))
        .matcher(Matcher::rooted_member_call(
            "app.plugins.disablePluginAndSave",
        ))
        .build()
        .unwrap()
}
