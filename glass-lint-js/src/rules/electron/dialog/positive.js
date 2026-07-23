// @case description positive fixture for electron:electron.dialog
// @tool glass-lint rules=electron:electron.dialog
import * as electron from "electron";

// Proven Electron namespace calls.
// @expect-error glass-lint rule=electron:electron.dialog
electron.dialog.showOpenDialog({ properties: ["openFile"] });
// @expect-error glass-lint rule=electron:electron.dialog
electron.dialog.showSaveDialog({});
// @expect-error glass-lint rule=electron:electron.dialog
electron.dialog.showOpenDialogSync({});
// @expect-error glass-lint rule=electron:electron.dialog
electron.dialog.showSaveDialogSync({});
// @expect-error glass-lint rule=electron:electron.dialog
electron.dialog.showMessageBox({});
// @expect-error glass-lint rule=electron:electron.dialog
electron.dialog.showMessageBoxSync({});
// @expect-error glass-lint rule=electron:electron.dialog
electron.dialog.showErrorBox("title", "content");
// @expect-error glass-lint rule=electron:electron.dialog
electron.dialog.showCertificateTrustDialog({});

// A direct namespace alias retains module provenance.
const electronAlias = electron;
// @expect-error glass-lint rule=electron:electron.dialog
electronAlias.dialog.showOpenDialog({});

// CommonJS namespace provenance, including a direct load.
const electronCjs = require("electron");
// @expect-error glass-lint rule=electron:electron.dialog
electronCjs.dialog.showOpenDialog({});
// @expect-error glass-lint rule=electron:electron.dialog
require("electron").dialog.showSaveDialog({});

// Static interop wrappers preserve the underlying module namespace.
const electronInterop = __toESM(require("electron"));
// @expect-error glass-lint rule=electron:electron.dialog
electronInterop.dialog.showOpenDialog({});
