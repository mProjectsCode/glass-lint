// @case description positive fixture for js:electron.ipc
// @tool glass-lint rules=js:electron.ipc
import * as electron from "electron";

// Proven namespace calls.
// @expect-error glass-lint rule=js:electron.ipc message_id=detected
electron.ipcRenderer.send("x");
// @expect-error glass-lint rule=js:electron.ipc message_id=detected
electron.ipcRenderer.invoke("second");

// Namespace aliases retain module provenance.
const electronAlias = electron;
// @expect-error glass-lint rule=js:electron.ipc message_id=detected
electronAlias.ipcRenderer.send("aliased");

// CommonJS and static interop namespace loads are also supported.
const electronCjs = require("electron");
// @expect-error glass-lint rule=js:electron.ipc message_id=detected
electronCjs.ipcRenderer.invoke("commonjs");
require("electron").ipcRenderer.send("direct");
const electronInterop = __toESM(require("electron"));
// @expect-error glass-lint rule=js:electron.ipc message_id=detected
electronInterop.ipcRenderer.invoke("interop");
