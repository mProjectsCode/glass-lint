use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.menu")
        .label("Uses Obsidian menus")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("menu.addMenuItem"))
        .build()
        .unwrap()
}
