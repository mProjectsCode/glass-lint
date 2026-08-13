//! Obsidian CLI-handler registration rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects `Plugin.registerCliHandler` on a proven Obsidian plugin instance.
/// Proven receiver and callable aliases retain identity; local lookalikes,
/// dynamic members, and reassigned aliases remain fail-closed.
pub fn rule() -> Rule {
    Rule::catalog_builder("cli.register")
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
