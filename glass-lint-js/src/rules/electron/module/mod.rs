//! Electron module-boundary rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

const MODULE_CALLS: &[&str] = &[
    "webContents.fromId",
    "webContents.fromFrame",
    "webContents.getAllWebContents",
    "webContents.getFocusedWebContents",
    "session.fromPartition",
    "contextBridge.exposeInMainWorld",
    "contextBridge.exposeInIsolatedWorld",
    "globalShortcut.register",
    "globalShortcut.unregister",
    "globalShortcut.registerAll",
    "globalShortcut.unregisterAll",
    "globalShortcut.isRegistered",
    "desktopCapturer.getSources",
    "powerMonitor.getSystemIdleState",
    "powerMonitor.getSystemIdleTime",
    "powerMonitor.getSystemMemoryInfo",
    "powerMonitor.getCPUUsage",
    "powerMonitor.getIOCounters",
    "app.getPath",
    "app.getVersion",
    "app.getAppPath",
    "app.getName",
    "app.getLocale",
    "app.quit",
    "app.whenReady",
    "clipboard.readText",
    "clipboard.writeText",
    "clipboard.clear",
    "clipboard.readHTML",
    "clipboard.readImage",
    "clipboard.writeHTML",
    "clipboard.writeImage",
    "clipboard.availableFormats",
    "safeStorage.encryptString",
    "safeStorage.decryptString",
    "safeStorage.isEncryptionAvailable",
    "screen.getAllDisplays",
    "screen.getPrimaryDisplay",
    "screen.getCursorScreenPoint",
    "screen.getDisplayNearestPoint",
    "screen.getDisplayMatching",
    "protocol.registerFileProtocol",
    "protocol.registerStringProtocol",
    "protocol.registerHttpProtocol",
    "protocol.registerBufferProtocol",
    "protocol.unregisterProtocol",
    "nativeImage.createFromPath",
    "nativeImage.createFromBuffer",
    "nativeImage.createFromDataURL",
    "nativeImage.createEmpty",
    "BrowserWindow.getAllWindows",
    "BrowserWindow.getFocusedWindow",
];

const MODULE_READS: &[&str] = &[
    "nativeTheme.shouldUseDarkColors",
    "app.isPackaged",
    "session.defaultSession",
];

/// Detects imports and unshadowed static CommonJS/interop loads of the exact
/// `electron` module, plus calls and reads from several high-impact Electron
/// exports. Module-provenance behavior matchers reject similarly named modules,
/// shadowed `require` calls, and local lookalikes.
pub fn rule() -> Rule {
    let mut builder = Rule::builder("electron.module")
        .description("Uses Electron APIs")
        .category(Category::new("electron/module").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::import_exact("electron"))
        .query(QueryDecl::import_exact("electron/main"))
        .query(QueryDecl::import_exact("electron/renderer"))
        .query(QueryDecl::import_exact("electron/common"))
        .query(QueryDecl::import_exact("electron/utility"))
        .query(QueryDecl::import_exact("electron/sandbox"))
        .query(QueryDecl::constructor_module("electron", "BrowserWindow"));

    for member in MODULE_CALLS {
        builder = builder.query(QueryDecl::member_call_module("electron", *member));
    }
    for member in MODULE_READS {
        builder = builder.query(QueryDecl::member_read_module("electron", *member));
    }

    builder.build().unwrap()
}
