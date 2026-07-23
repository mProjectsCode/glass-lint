// @case description negative fixture for electron:electron.dialog
// @tool glass-lint rules=electron:electron.dialog
const dialog = {
  showOpenDialog() {},
  showSaveDialog() {},
};

// A same-shaped local object is not Electron.
// @expect-no-error glass-lint rule=electron:electron.dialog
dialog.showOpenDialog({});

// Similar module names are not Electron.
// @expect-no-error glass-lint rule=electron:electron.dialog
require("other-electron").dialog.showSaveDialog({});

// Inline CommonJS member chains share module provenance.
// @expect-error glass-lint rule=electron:electron.dialog
require("electron").dialog.showSaveDialog({});

// Reassignment drops module provenance from a namespace alias.
let reassigned = require("electron");
reassigned = {};
// @expect-no-error glass-lint rule=electron:electron.dialog
reassigned.dialog.showSaveDialog({});

// A shadowed CommonJS loader is not a module load.
function shadowed(require) {
  const localElectron = require("electron");
  // @expect-no-error glass-lint rule=electron:electron.dialog
  localElectron.dialog.showOpenDialog({});
}
shadowed(() => ({}));

// A local binding named electron is also not the module namespace.
const localElectron = { dialog: { showSaveDialog() {} } };
// @expect-no-error glass-lint rule=electron:electron.dialog
localElectron.dialog.showSaveDialog({});
