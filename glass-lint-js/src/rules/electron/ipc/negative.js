// @case description negative fixture for electron:electron.ipc
// @tool glass-lint rules=electron:electron.ipc
// Same-shaped local objects are not Electron.
const electron = { ipcRenderer: { send() {} } };
// @expect-no-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.send("local");
// @expect-no-error glass-lint rule=electron:electron.ipc
electron.ipcRenderer.off("local", handler);
// A local webContents-shaped object is not Electron.
// @expect-no-error glass-lint rule=electron:electron.ipc
electron.webContents.send("local", payload);
// @expect-no-error glass-lint rule=electron:electron.ipc
electron.webContents.removeAllListeners("local");

// Reassignment drops module provenance from an alias.
let reassigned = require("electron");
reassigned = {};
// @expect-no-error glass-lint rule=electron:electron.ipc
reassigned.ipcRenderer.invoke("reassigned");
// @expect-no-error glass-lint rule=electron:electron.ipc
reassigned.webContents.send("reassigned", payload);

// Shadowing require prevents a local object from becoming a module alias.
function shadowed(require) {
  const localElectron = require("electron");
  // @expect-no-error glass-lint rule=electron:electron.ipc
  localElectron.ipcRenderer.send("shadowed");
}
shadowed(() => ({}));

// Inline CommonJS member chains share module provenance.
// @expect-error glass-lint rule=electron:electron.ipc
require("electron").ipcRenderer.send("inline");

// A same-named helper is unrelated to Electron IPC.
function localLookalike() { return null; }
// @expect-no-error glass-lint rule=electron:electron.ipc
localLookalike();

// Unlisted or local frame-shaped methods do not establish Electron IPC.
const localFrame = { send() {} };
// @expect-no-error glass-lint rule=electron:electron.ipc
localFrame.send("local");
