//! Obsidian settings-tab rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects syntactic `this.addSettingTab()` registration calls and
/// `PluginSettingTab` constructors/subclasses. The registration form requires
/// a proven Obsidian `Plugin` receiver and does not follow aliases or
/// reassignment; constructor forms follow ESM, namespace, and CommonJS
/// `obsidian` provenance, while arguments and class bodies are ignored.
pub fn rule() -> Rule {
    Rule::builder("ui.settings-tab")
        .description("Registers plugin settings UI")
        .category(Category::new("ui").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance("obsidian", "Plugin", "addSettingTab"))
        .query(QueryDecl::constructor_heuristic("PluginSettingTab"))
        .query(QueryDecl::constructor_module("obsidian", "PluginSettingTab"))
        .query(QueryDecl::class_module("obsidian", "PluginSettingTab"))
        .build()
        .unwrap()
}
