//! Obsidian status-bar registration rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects the syntactic `this.addStatusBarItem()` registration call,
/// including a static computed property. The instance matcher requires a
/// proven Obsidian `Plugin` receiver and does not follow aliases/reassignment;
/// other receivers, dynamic properties, and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("ui.status-bar")
        .description("Registers status bar items")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "addStatusBarItem")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
