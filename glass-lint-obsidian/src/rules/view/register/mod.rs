//! Obsidian view-registration rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `this.registerView()` call, including a statically
/// computed property name. The instance matcher requires a proven Obsidian
/// `Plugin` receiver and does not follow aliases or reassignment; other
/// receivers, dynamic properties, and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("view.register")
        .label("Registers Obsidian views")
        .category("view")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerView",
        ))
        .build()
        .unwrap()
}
