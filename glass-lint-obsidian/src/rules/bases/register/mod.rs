//! Obsidian Bases view-registration rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects `Plugin.registerBasesView` on a proven Obsidian plugin instance.
/// Local lookalikes, dynamic members, and callable aliases remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("bases.register")
        .description("Registers an Obsidian Bases view")
        .category("bases")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "registerBasesView")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
