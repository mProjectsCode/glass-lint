//! Electron IPC rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

const RECEIVERS: &[(&str, &[&str])] = &[
    (
        "ipcRenderer",
        &[
            "send",
            "invoke",
            "sendSync",
            "postMessage",
            "sendToHost",
            "on",
            "once",
            "addListener",
            "removeListener",
            "off",
            "removeAllListeners",
        ],
    ),
    (
        "ipcMain",
        &[
            "on",
            "once",
            "handle",
            "handleOnce",
            "removeHandler",
            "removeListener",
            "off",
            "removeAllListeners",
        ],
    ),
    (
        "webContents",
        &[
            "send",
            "sendToFrame",
            "postMessage",
            "on",
            "once",
            "removeListener",
            "off",
            "removeAllListeners",
        ],
    ),
    (
        "webFrameMain",
        &[
            "send",
            "postMessage",
            "on",
            "once",
            "removeListener",
            "off",
            "removeAllListeners",
        ],
    ),
];

/// Detects Electron renderer IPC send, invoke, listener, and cleanup calls,
/// plus `ipcMain` registration/handling and cleanup, through a receiver proven
/// to be the `electron` module namespace. Namespace aliases,
/// direct unshadowed CommonJS loads, and static interop wrappers are followed;
/// local lookalikes, shadowed `require`, and reassigned aliases are excluded.
/// WebContents and WebFrameMain message calls are included; inline
/// `require("electron").ipcRenderer` chains and other IPC methods or reads are
/// outside this call-only rule.
pub fn rule() -> Rule {
    let mut builder = Rule::builder("electron.ipc")
        .description("Uses Electron IPC APIs")
        .category("electron/ipc")
        .severity(Severity::Info)
        .confidence(Confidence::High);
    for &(receiver, methods) in RECEIVERS {
        for &method in methods {
            builder = builder.declaration(
                MatcherDecl::builder()
                    .member_call_module("electron", format!("{receiver}.{method}"))
                    .build()
                    .expect("valid matcher declaration"),
            );
        }
    }
    builder.build().unwrap()
}
