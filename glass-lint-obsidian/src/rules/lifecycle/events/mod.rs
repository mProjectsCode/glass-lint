//! Obsidian lifecycle-registration rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic lifecycle-registration chains
/// `this.registerEvent`, `this.registerDomEvent`, and `this.registerInterval`.
/// The instance matcher requires a proven Obsidian `Plugin` receiver and
/// accepts static computed names; aliases, reassignment, dynamic properties,
/// and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("lifecycle.events")
        .description("Registers Obsidian lifecycle events")
        .category("lifecycle")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerEvent",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerDomEvent",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerInterval",
        ))
        .build()
        .unwrap()
}
