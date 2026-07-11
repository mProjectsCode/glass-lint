// @case description negative fixture for js:electron.ipc
// @tool glass-lint rules=js:electron.ipc
// Same-shaped local objects are not Electron.
const electron = { ipcRenderer: { send() {} } };
// @expect-no-error glass-lint rule=js:electron.ipc message_id=detected
electron.ipcRenderer.send("local");

// Reassignment drops module provenance from an alias.
let reassigned = require("electron");
reassigned = {};
// @expect-no-error glass-lint rule=js:electron.ipc message_id=detected
reassigned.ipcRenderer.invoke("reassigned");

// Shadowing require prevents a local object from becoming a module alias.
function shadowed(require) {
  const localElectron = require("electron");
  // @expect-no-error glass-lint rule=js:electron.ipc message_id=detected
  localElectron.ipcRenderer.send("shadowed");
}
shadowed(() => ({}));

// Inline CommonJS member chains share module provenance.
// @expect-error glass-lint rule=js:electron.ipc message_id=detected
require("electron").ipcRenderer.send("inline");

// A same-named helper is unrelated to Electron IPC.
function localLookalike() { return null; }
// @expect-no-error glass-lint rule=js:electron.ipc message_id=detected
localLookalike();
