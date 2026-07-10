use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects Electron `shell.openExternal` and `shell.openPath` calls through a
/// proven `electron` module namespace. ESM/CommonJS namespace aliases and
/// static interop wrappers retain module provenance, while local lookalikes,
/// shadowed `require`, and reassigned aliases do not. Inline
/// `require("electron").shell` chains, unlisted shell methods, and non-call
/// reads are intentionally outside the rule.
pub(crate) fn rule() -> Rule {
    Rule::builder("electron.shell")
        .label("Uses Electron shell APIs")
        .category("electron/shell")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .matcher(Matcher::module_member_call(
            "electron",
            "shell.openExternal",
        ))
        .matcher(Matcher::module_member_call("electron", "shell.openPath"))
        .build()
        .unwrap()
}
