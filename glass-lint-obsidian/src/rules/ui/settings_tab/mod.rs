//! Obsidian settings-tab rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, QueryDecl, Rule, Severity};

/// Detects syntactic `this.addSettingTab()` registration calls and
/// `PluginSettingTab` constructors/subclasses. The registration form requires
/// a proven Obsidian `Plugin` receiver and does not follow aliases or
/// reassignment; strict constructor and class forms follow ESM, namespace,
/// and CommonJS `obsidian` provenance, while the heuristic constructor form
/// uses static spelling. Arguments and class bodies are ignored.
pub fn rule() -> Rule {
    Rule::builder("ui.settings-tab")
        .description("Registers plugin settings UI")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian",
            "Plugin",
            "addSettingTab",
        ))
        .query(EventQuery::constructor_heuristic("PluginSettingTab"))
        .query(EventQuery::constructor_module(
            "obsidian",
            "PluginSettingTab",
        ))
        .query(EventQuery::class_module("obsidian", "PluginSettingTab"))
        .build()
        .unwrap()
}
