//! Obsidian editor-content access rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects content reads and mutations on proven Obsidian `Editor` instances.
/// Static computed method names are accepted; local lookalikes, dynamic
/// properties, aliases, and reassigned receivers remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("editor.content")
        .description("Reads or changes Obsidian editor content")
        .category("editor")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::instance_member_call(
            "obsidian", "Editor", "getValue",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian", "Editor", "setValue",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian", "Editor", "getLine",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian", "Editor", "setLine",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian", "Editor", "getRange",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Editor",
            "replaceRange",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Editor",
            "getSelection",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Editor",
            "replaceSelection",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Editor",
            "getCursor",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Editor",
            "setCursor",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Editor",
            "setSelection",
        ))
        .declaration(MatcherDecl::instance_member_call(
            "obsidian",
            "Editor",
            "setSelections",
        ))
        .build()
        .unwrap()
}
