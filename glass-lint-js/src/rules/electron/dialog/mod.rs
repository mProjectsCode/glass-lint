use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
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
