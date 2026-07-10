use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.settings-tab")
        .label("Registers plugin settings UI")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("this.addSettingTab"))
        .matcher(Matcher::heuristic_constructor("PluginSettingTab"))
        .matcher(Matcher::module_constructor("obsidian", "PluginSettingTab"))
        .matcher(Matcher::module_class("obsidian", "PluginSettingTab"))
        .build()
        .unwrap()
}
