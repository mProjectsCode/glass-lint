use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects syntactic `this.addSettingTab()` registration calls and
/// `PluginSettingTab` constructors/subclasses. The registration form is a
/// heuristic that does not prove an Obsidian receiver or follow aliases and
/// reassignment; constructor forms follow ESM, namespace, and CommonJS
/// `obsidian` provenance, while arguments and class bodies are ignored.
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.settings-tab")
        .label("Registers plugin settings UI")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "addSettingTab",
        ))
        .matcher(Matcher::heuristic_constructor("PluginSettingTab"))
        .matcher(Matcher::module_constructor("obsidian", "PluginSettingTab"))
        .matcher(Matcher::module_class("obsidian", "PluginSettingTab"))
        .build()
        .unwrap()
}
