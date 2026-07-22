//! Obsidian lifecycle-registration rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects the syntactic lifecycle-registration chains
/// `this.registerEvent`, `this.registerDomEvent`, `this.registerInterval`, and
/// `this.registerObsidianProtocolHandler`. Bases and CLI registration have
/// dedicated provider rules.
/// The instance matcher requires a proven Obsidian `Plugin` receiver and
/// accepts static computed names; aliases, reassignment, dynamic properties,
/// and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("lifecycle.events")
        .description("Registers Obsidian lifecycle events")
        .category("lifecycle")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Plugin",
            "registerEvent",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Plugin",
            "registerDomEvent",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Plugin",
            "registerInterval",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Plugin",
            "registerObsidianProtocolHandler",
        ))
        .build()
        .unwrap()
}
