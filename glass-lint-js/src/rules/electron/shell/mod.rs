use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

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
