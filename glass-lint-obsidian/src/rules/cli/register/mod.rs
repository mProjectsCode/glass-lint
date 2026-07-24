//! Obsidian CLI-handler registration rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects `Plugin.registerCliHandler` on a proven Obsidian plugin instance.
/// Local lookalikes, dynamic members, and callable aliases remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("cli.register")
        .description("Registers an Obsidian CLI handler")
        .category(Category::new("cli").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "registerCliHandler")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
