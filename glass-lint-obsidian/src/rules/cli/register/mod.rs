//! Obsidian CLI-handler registration rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects `Plugin.registerCliHandler` on a proven Obsidian plugin instance.
/// Local lookalikes, dynamic members, and callable aliases remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("cli.register")
        .description("Registers an Obsidian CLI handler")
        .category("cli")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerCliHandler",
        ))
        .build()
        .unwrap()
}
