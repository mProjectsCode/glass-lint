use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("electron.ipc")
        .label("Uses Electron IPC APIs")
        .category("electron/ipc")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::module_member_call("electron", "ipcRenderer.send"))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcRenderer.invoke",
        ))
        .build()
        .unwrap()
}
