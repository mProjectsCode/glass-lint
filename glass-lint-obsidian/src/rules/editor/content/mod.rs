//! Obsidian editor-content access rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects content reads and mutations on proven Obsidian `Editor` instances.
/// Static computed method names are accepted; local lookalikes, dynamic
/// properties, aliases, and reassigned receivers remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("editor.content")
        .description("Reads or changes Obsidian editor content")
        .category(Category::new("editor").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "getValue")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "setValue")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "getLine")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "setLine")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "getRange")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "replaceRange")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "getSelection")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "replaceSelection")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "getCursor")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "setCursor")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "setSelection")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Editor", "setSelections")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
