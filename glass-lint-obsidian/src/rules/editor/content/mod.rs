//! Obsidian editor-content access rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects content reads and mutations on proven Obsidian `Editor` instances.
/// Static computed method names are accepted; local lookalikes, dynamic
/// properties, aliases, and reassigned receivers remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("editor.content")
        .description("Reads or changes Obsidian editor content")
        .category(Category::new("editor").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "getValue"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "setValue"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "getLine"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "setLine"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "getRange"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "replaceRange"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "getSelection"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "replaceSelection"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "getCursor"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "setCursor"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "setSelection"))
        .query(QueryDecl::member_call_instance("obsidian", "Editor", "setSelections"))
        .build()
        .unwrap()
}
