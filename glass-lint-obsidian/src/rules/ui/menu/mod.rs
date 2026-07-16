//! Obsidian menu rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `menu.addMenuItem()` call. This medium-confidence
/// heuristic does not prove that `menu` is an Obsidian menu and does not follow
/// aliases, shadowing, or reassignment; static computed names resolve, while
/// other receivers, dynamic properties, and near-name methods do not. Menu
/// item arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("ui.menu")
        .label("Uses Obsidian menus")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("menu.addMenuItem"))
        .build()
        .unwrap()
}
