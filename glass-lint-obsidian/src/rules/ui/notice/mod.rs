use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.notice")
        .label("Uses Obsidian notices")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_constructor("Notice"))
        .matcher(Matcher::module_constructor("obsidian", "Notice"))
        .matcher(Matcher::module_class("obsidian", "Notice"))
        .build()
        .unwrap()
}
