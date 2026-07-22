//! Electron native-dialog rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects calls to Electron's `dialog.showOpenDialog` and
/// `dialog.showSaveDialog` when the receiver has proven `electron` module
/// namespace provenance. ESM/CommonJS namespace aliases and static interop
/// wrappers retain that provenance; local lookalikes, shadowed `require`, and
/// reassigned aliases do not. Inline `require("electron").dialog` chains are
/// not followed, and the rule reports the call rather than a later read or an
/// unlisted dialog method.
pub fn rule() -> Rule {
    Rule::builder("electron.dialog")
        .description("Uses Electron native dialogs")
        .category("electron/dialog")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::module_member_call(
            "electron",
            "dialog.showOpenDialog",
        ))
        .declaration(MatcherDecl::module_member_call(
            "electron",
            "dialog.showSaveDialog",
        ))
        .declaration(MatcherDecl::module_member_call(
            "electron",
            "dialog.showOpenDialogSync",
        ))
        .declaration(MatcherDecl::module_member_call(
            "electron",
            "dialog.showSaveDialogSync",
        ))
        .declaration(MatcherDecl::module_member_call(
            "electron",
            "dialog.showMessageBox",
        ))
        .declaration(MatcherDecl::module_member_call(
            "electron",
            "dialog.showMessageBoxSync",
        ))
        .declaration(MatcherDecl::module_member_call(
            "electron",
            "dialog.showErrorBox",
        ))
        .declaration(MatcherDecl::module_member_call(
            "electron",
            "dialog.showCertificateTrustDialog",
        ))
        .build()
        .unwrap()
}
