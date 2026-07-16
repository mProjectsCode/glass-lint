//! Obsidian command-registration rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `this.addCommand()` registration call, including a
/// statically computed `addCommand` property. The instance matcher requires a
/// proven Obsidian `Plugin` receiver and does not follow aliases, shadowing, or
/// reassignment; other receivers and dynamic properties are excluded.
pub fn rule() -> Rule {
    Rule::builder("ui.command")
        .label("Registers commands")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "addCommand",
        ))
        .build()
        .unwrap()
}
