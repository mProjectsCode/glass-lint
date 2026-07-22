//! Obsidian plugin enable/disable rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted plugin-manager calls that change another plugin's enabled
/// state.
pub fn rule() -> Rule {
    Rule::builder("plugins.enable-disable")
        .description("Enables or disables other plugins")
        .category("plugins")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call("app.plugins.enablePlugin"))
        .declaration(MatcherDecl::rooted_member_call("app.plugins.disablePlugin"))
        .declaration(MatcherDecl::rooted_member_call(
            "app.plugins.enablePluginAndSave",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.plugins.disablePluginAndSave",
        ))
        .build()
        .unwrap()
}
