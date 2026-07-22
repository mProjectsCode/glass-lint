//! Obsidian settings-tab rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects syntactic `this.addSettingTab()` registration calls and
/// `PluginSettingTab` constructors/subclasses. The registration form requires
/// a proven Obsidian `Plugin` receiver and does not follow aliases or
/// reassignment; constructor forms follow ESM, namespace, and CommonJS
/// `obsidian` provenance, while arguments and class bodies are ignored.
pub fn rule() -> Rule {
    Rule::builder("ui.settings-tab")
        .description("Registers plugin settings UI")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "addSettingTab")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_heuristic("PluginSettingTab")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_module("obsidian", "PluginSettingTab")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .class_module("obsidian", "PluginSettingTab")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
