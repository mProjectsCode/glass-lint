// @case description positive fixture for js:electron.dialog
// @tool glass-lint rules=js:electron.dialog
import * as electron from "electron";
// @expect-error glass-lint rule=js:electron.dialog message_id=detected
electron.dialog.showOpenDialog({ properties: ["openFile"] });
// @expect-error glass-lint rule=js:electron.dialog message_id=detected
electron.dialog.showSaveDialog({});
// CommonJS namespace provenance
const electronCjs = require("electron");

// @expect-error glass-lint rule=js:electron.dialog message_id=detected
electronCjs.dialog.showOpenDialog({});
