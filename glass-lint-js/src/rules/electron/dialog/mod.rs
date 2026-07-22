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
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "dialog.showOpenDialog")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "dialog.showSaveDialog")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "dialog.showOpenDialogSync")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "dialog.showSaveDialogSync")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "dialog.showMessageBox")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "dialog.showMessageBoxSync")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "dialog.showErrorBox")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("electron", "dialog.showCertificateTrustDialog")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
