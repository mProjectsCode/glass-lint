//! Obsidian status-bar registration rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects the syntactic `this.addStatusBarItem()` registration call,
/// including a static computed property. The instance matcher requires a
/// proven Obsidian `Plugin` receiver and follows proven receiver and callable
/// aliases; reassignment, other receivers, dynamic properties, and near-name
/// methods are excluded.
pub fn rule() -> Rule {
    Rule::catalog_builder("ui.status-bar")
        .description("Registers status bar items")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian",
            "Plugin",
            "addStatusBarItem",
        ))
        .build()
        .unwrap()
}
