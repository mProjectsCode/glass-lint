// @case description positive fixture for electron:electron.ipc
// @tool glass-lint rules=electron:electron.ipc
import * as electron from "electron";

// Proven namespace calls.
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.send("x");
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.invoke("second");
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.sendSync("sync");
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.postMessage("post", {});
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.sendToHost("host");
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.on("event", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.once("once-event", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.addListener("listener", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.removeListener("listener", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.off("listener", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcRenderer.removeAllListeners("listener");
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcMain.on("channel", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcMain.once("once", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcMain.handle("handle", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcMain.handleOnce("handle-once", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcMain.removeHandler("handle");
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcMain.removeListener("channel", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcMain.off("channel", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.ipcMain.removeAllListeners("channel");

// WebContents and WebFrameMain can carry IPC messages to renderer frames.
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webContents.send("channel", payload);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webContents.sendToFrame(frameId, "channel", payload);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webContents.postMessage("channel", payload, transfer);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webContents.on("ipc-message", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webContents.once("ipc-message", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webContents.removeListener("ipc-message", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webContents.off("ipc-message", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webContents.removeAllListeners("ipc-message");
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webFrameMain.send("channel", payload);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webFrameMain.postMessage("channel", payload, transfer);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webFrameMain.on("dom-ready", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webFrameMain.once("dom-ready", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webFrameMain.removeListener("dom-ready", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webFrameMain.off("dom-ready", handler);
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electron.webFrameMain.removeAllListeners("dom-ready");

// Namespace aliases retain module provenance.
const electronAlias = electron;
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electronAlias.ipcRenderer.send("aliased");

// CommonJS and static interop namespace loads are also supported.
const electronCjs = require("electron");
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electronCjs.ipcRenderer.invoke("commonjs");
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
require("electron").ipcRenderer.send("direct");
const electronInterop = __toESM(require("electron"));
// @expect-error glass-lint rule=electron:electron.ipc message_id=detected
electronInterop.ipcRenderer.invoke("interop");
