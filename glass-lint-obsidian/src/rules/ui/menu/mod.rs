//! Obsidian menu rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects proven `obsidian.Menu` instance calls. Unproven callback parameters,
/// aliases, and same-shaped local receivers are excluded.
pub fn rule() -> Rule {
    Rule::builder("ui.menu")
        .description("Uses Obsidian menus")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::instance_member_call(
            "obsidian", "Menu", "addItem",
        ))
        .build()
        .unwrap()
}
