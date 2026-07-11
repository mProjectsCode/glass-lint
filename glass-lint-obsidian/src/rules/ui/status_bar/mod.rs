use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `this.addStatusBarItem()` registration call,
/// including a static computed property. It does not prove an Obsidian
/// receiver or follow aliases/reassignment; other receivers, dynamic
/// properties, and near-name methods are excluded.
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.status-bar")
        .label("Registers status bar items")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "addStatusBarItem",
        ))
        .build()
        .unwrap()
}
