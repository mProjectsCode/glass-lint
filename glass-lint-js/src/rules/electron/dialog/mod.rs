use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects calls to Electron's `dialog.showOpenDialog` and
/// `dialog.showSaveDialog` when the receiver has proven `electron` module
/// namespace provenance. ESM/CommonJS namespace aliases and static interop
/// wrappers retain that provenance; local lookalikes, shadowed `require`, and
/// reassigned aliases do not. Inline `require("electron").dialog` chains are
/// not followed, and the rule reports the call rather than a later read or an
/// unlisted dialog method.
pub fn rule() -> Rule {
    Rule::builder("electron.dialog")
        .label("Uses Electron native dialogs")
        .category("electron/dialog")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::module_member_call(
            "electron",
            "dialog.showOpenDialog",
        ))
        .matcher(Matcher::module_member_call(
            "electron",
            "dialog.showSaveDialog",
        ))
        .build()
        .unwrap()
}
