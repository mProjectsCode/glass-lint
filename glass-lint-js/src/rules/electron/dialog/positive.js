// @case description positive fixture for js:electron.dialog
// @tool glass-lint rules=js:electron.dialog
import * as electron from "electron";

// Proven Electron namespace calls.
// @expect-error glass-lint rule=js:electron.dialog message_id=detected
electron.dialog.showOpenDialog({ properties: ["openFile"] });
// @expect-error glass-lint rule=js:electron.dialog message_id=detected
electron.dialog.showSaveDialog({});

// A direct namespace alias retains module provenance.
const electronAlias = electron;
// @expect-error glass-lint rule=js:electron.dialog message_id=detected
electronAlias.dialog.showOpenDialog({});

// CommonJS namespace provenance, including a direct load.
const electronCjs = require("electron");
// @expect-error glass-lint rule=js:electron.dialog message_id=detected
electronCjs.dialog.showOpenDialog({});
// @expect-error glass-lint rule=js:electron.dialog message_id=detected
require("electron").dialog.showSaveDialog({});

// Static interop wrappers preserve the underlying module namespace.
const electronInterop = __toESM(require("electron"));
// @expect-error glass-lint rule=js:electron.dialog message_id=detected
electronInterop.dialog.showOpenDialog({});
