//! Obsidian command-registration rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects `this.addCommand()` registrations, including static computed
/// properties and bounded extracted aliases. The instance matcher requires a
/// proven Obsidian `Plugin` receiver; shadowing, reassignment, and dynamic
/// properties are excluded.
pub fn rule() -> Rule {
    Rule::builder("ui.command")
        .description("Registers commands")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian",
            "Plugin",
            "addCommand",
        ))
        .build()
        .unwrap()
}
