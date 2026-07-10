use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.modal")
        .label("Uses Obsidian modal UI")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_constructor("Modal"))
        .matcher(Matcher::module_constructor("obsidian", "Modal"))
        .matcher(Matcher::module_class("obsidian", "Modal"))
        .build()
        .unwrap()
}
