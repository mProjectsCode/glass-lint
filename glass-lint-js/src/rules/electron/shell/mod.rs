//! Electron shell rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects Electron `shell.openExternal` and `shell.openPath` calls through a
/// proven `electron` module namespace. ESM/CommonJS namespace aliases and
/// static interop wrappers retain module provenance, while local lookalikes,
/// shadowed `require`, and reassigned aliases do not. Inline
/// `require("electron").shell` chains, unlisted shell methods, and non-call
/// reads are intentionally outside the rule.
pub fn rule() -> Rule {
    Rule::builder("electron.shell")
        .description("Uses Electron shell APIs")
        .category(Category::new("electron/shell").unwrap())
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "shell.openExternal")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "shell.openPath")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "shell.showItemInFolder")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "shell.trashItem")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "shell.beep")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "shell.readShortcutLink")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "shell.writeShortcutLink")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
