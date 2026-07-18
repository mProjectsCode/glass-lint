//! Obsidian editor-content access rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects content reads and mutations on proven Obsidian `Editor` instances.
/// Static computed method names are accepted; local lookalikes, dynamic
/// properties, aliases, and reassigned receivers remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("editor.content")
        .description("Reads or changes Obsidian editor content")
        .category("editor")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian", "Editor", "getValue",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian", "Editor", "setValue",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian", "Editor", "getLine",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian", "Editor", "setLine",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian", "Editor", "getRange",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Editor",
            "replaceRange",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Editor",
            "getSelection",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Editor",
            "replaceSelection",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Editor",
            "getCursor",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Editor",
            "setCursor",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Editor",
            "setSelection",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Editor",
            "setSelections",
        ))
        .build()
        .unwrap()
}
