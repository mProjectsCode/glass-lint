//! Electron native-dialog rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

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
        .category(Category::new("electron/dialog").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_module("electron", "dialog.showOpenDialog"))
        .query(QueryDecl::member_call_module("electron", "dialog.showSaveDialog"))
        .query(QueryDecl::member_call_module("electron", "dialog.showOpenDialogSync"))
        .query(QueryDecl::member_call_module("electron", "dialog.showSaveDialogSync"))
        .query(QueryDecl::member_call_module("electron", "dialog.showMessageBox"))
        .query(QueryDecl::member_call_module("electron", "dialog.showMessageBoxSync"))
        .query(QueryDecl::member_call_module("electron", "dialog.showErrorBox"))
        .query(QueryDecl::member_call_module("electron", "dialog.showCertificateTrustDialog"))
        .build()
        .unwrap()
}
