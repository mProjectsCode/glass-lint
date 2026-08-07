//! Electron shell rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects Electron `shell.openExternal` and `shell.openPath` calls through a
/// proven `electron` module namespace. ESM/CommonJS namespace aliases and
/// static interop wrappers retain module provenance, while local lookalikes,
/// shadowed `require`, and reassigned aliases do not. Inline
/// `require("electron").shell` chains, unlisted shell methods, and non-call
/// reads are intentionally outside the rule.
pub fn rule() -> Rule {
    Rule::builder("electron.shell")
        .description("Uses Electron shell APIs")
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .query(EventQuery::member_call_module(
            "electron",
            "shell.openExternal",
        ))
        .query(EventQuery::member_call_module("electron", "shell.openPath"))
        .query(EventQuery::member_call_module(
            "electron",
            "shell.showItemInFolder",
        ))
        .query(EventQuery::member_call_module(
            "electron",
            "shell.trashItem",
        ))
        .query(EventQuery::member_call_module("electron", "shell.beep"))
        .query(EventQuery::member_call_module(
            "electron",
            "shell.readShortcutLink",
        ))
        .query(EventQuery::member_call_module(
            "electron",
            "shell.writeShortcutLink",
        ))
        .build()
        .unwrap()
}
