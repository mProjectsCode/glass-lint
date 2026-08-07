//! Obsidian CLI-handler registration rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects `Plugin.registerCliHandler` on a proven Obsidian plugin instance.
/// Local lookalikes, dynamic members, and callable aliases remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("cli.register")
        .description("Registers an Obsidian CLI handler")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian",
            "Plugin",
            "registerCliHandler",
        ))
        .build()
        .unwrap()
}
