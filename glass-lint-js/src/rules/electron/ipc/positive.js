// @case description positive fixture for electron:electron.ipc
// @tool glass-lint rules=electron:electron.ipc
import * as electron from "electron";

// Proven namespace calls.
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.send("x");
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.invoke("second");
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.sendSync("sync");
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.postMessage("post", {});
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.sendToHost("host");
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.on("event", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.once("once-event", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.addListener("listener", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.removeListener("listener", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.off("listener", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.removeAllListeners("listener");
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcMain.on("channel", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcMain.once("once", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcMain.handle("handle", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcMain.handleOnce("handle-once", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcMain.removeHandler("handle");
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcMain.removeListener("channel", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcMain.off("channel", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.ipcMain.removeAllListeners("channel");

// WebContents and WebFrameMain can carry IPC messages to renderer frames.
// @expect-error glass-lint rule=electron:electron.ipc
electron.webContents.send("channel", payload);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webContents.sendToFrame(frameId, "channel", payload);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webContents.postMessage("channel", payload, transfer);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webContents.on("ipc-message", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webContents.once("ipc-message", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webContents.removeListener("ipc-message", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webContents.off("ipc-message", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webContents.removeAllListeners("ipc-message");
// @expect-error glass-lint rule=electron:electron.ipc
electron.webFrameMain.send("channel", payload);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webFrameMain.postMessage("channel", payload, transfer);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webFrameMain.on("dom-ready", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webFrameMain.once("dom-ready", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webFrameMain.removeListener("dom-ready", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webFrameMain.off("dom-ready", handler);
// @expect-error glass-lint rule=electron:electron.ipc
electron.webFrameMain.removeAllListeners("dom-ready");

// Namespace aliases retain module provenance.
const electronAlias = electron;
// @expect-error glass-lint rule=electron:electron.ipc
electronAlias.ipcRenderer.send("aliased");

// CommonJS and static interop namespace loads are also supported.
const electronCjs = require("electron");
// @expect-error glass-lint rule=electron:electron.ipc
electronCjs.ipcRenderer.invoke("commonjs");
// @expect-error glass-lint rule=electron:electron.ipc
require("electron").ipcRenderer.send("direct");
const electronInterop = __toESM(require("electron"));
// @expect-error glass-lint rule=electron:electron.ipc
electronInterop.ipcRenderer.invoke("interop");
