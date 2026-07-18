//! Electron IPC rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects Electron renderer IPC send, invoke, listener, and cleanup calls,
/// plus `ipcMain` registration/handling and cleanup, through a receiver proven
/// to be the `electron` module namespace. Namespace aliases,
/// direct unshadowed CommonJS loads, and static interop wrappers are followed;
/// local lookalikes, shadowed `require`, and reassigned aliases are excluded.
/// WebContents and WebFrameMain message calls are included; inline
/// `require("electron").ipcRenderer` chains and other IPC methods or reads are
/// outside this call-only rule.
pub fn rule() -> Rule {
    Rule::builder("electron.ipc")
        .description("Uses Electron IPC APIs")
        .category("electron/ipc")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::module_member_call("electron", "ipcRenderer.send"))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcRenderer.invoke",
        ))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcRenderer.sendSync",
        ))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcRenderer.postMessage",
        ))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcRenderer.sendToHost",
        ))
        .matcher(Matcher::module_member_call("electron", "ipcRenderer.on"))
        .matcher(Matcher::module_member_call("electron", "ipcRenderer.once"))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcRenderer.addListener",
        ))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcRenderer.removeListener",
        ))
        .matcher(Matcher::module_member_call("electron", "ipcRenderer.off"))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcRenderer.removeAllListeners",
        ))
        .matcher(Matcher::module_member_call("electron", "ipcMain.on"))
        .matcher(Matcher::module_member_call("electron", "ipcMain.once"))
        .matcher(Matcher::module_member_call("electron", "ipcMain.handle"))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcMain.handleOnce",
        ))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcMain.removeHandler",
        ))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcMain.removeListener",
        ))
        .matcher(Matcher::module_member_call("electron", "ipcMain.off"))
        .matcher(Matcher::module_member_call(
            "electron",
            "ipcMain.removeAllListeners",
        ))
        .matcher(Matcher::module_member_call("electron", "webContents.send"))
        .matcher(Matcher::module_member_call(
            "electron",
            "webContents.sendToFrame",
        ))
        .matcher(Matcher::module_member_call(
            "electron",
            "webContents.postMessage",
        ))
        .matcher(Matcher::module_member_call("electron", "webContents.on"))
        .matcher(Matcher::module_member_call("electron", "webContents.once"))
        .matcher(Matcher::module_member_call(
            "electron",
            "webContents.removeListener",
        ))
        .matcher(Matcher::module_member_call("electron", "webContents.off"))
        .matcher(Matcher::module_member_call(
            "electron",
            "webContents.removeAllListeners",
        ))
        .matcher(Matcher::module_member_call("electron", "webFrameMain.send"))
        .matcher(Matcher::module_member_call(
            "electron",
            "webFrameMain.postMessage",
        ))
        .matcher(Matcher::module_member_call("electron", "webFrameMain.on"))
        .matcher(Matcher::module_member_call("electron", "webFrameMain.once"))
        .matcher(Matcher::module_member_call(
            "electron",
            "webFrameMain.removeListener",
        ))
        .matcher(Matcher::module_member_call("electron", "webFrameMain.off"))
        .matcher(Matcher::module_member_call(
            "electron",
            "webFrameMain.removeAllListeners",
        ))
        .build()
        .unwrap()
}
