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
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.plugins.enablePlugin")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.plugins.disablePlugin")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.plugins.enablePluginAndSave")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.plugins.disablePluginAndSave")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
