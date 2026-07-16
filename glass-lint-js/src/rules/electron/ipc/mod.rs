//! Electron IPC rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects Electron `ipcRenderer.send` and `ipcRenderer.invoke` calls through
/// a receiver proven to be the `electron` module namespace. Namespace aliases,
/// direct unshadowed CommonJS loads, and static interop wrappers are followed;
/// local lookalikes, shadowed `require`, and reassigned aliases are excluded.
/// Inline `require("electron").ipcRenderer` chains and other IPC methods or
/// reads are outside this call-only rule.
pub fn rule() -> Rule {
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
