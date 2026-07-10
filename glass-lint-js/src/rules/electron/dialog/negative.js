// @case description negative fixture for js:electron.dialog
// @tool glass-lint rules=js:electron.dialog
const dialog = {
  showOpenDialog() {},
  showSaveDialog() {},
};

// @expect-no-error glass-lint rule=js:electron.dialog message_id=detected
dialog.showOpenDialog({});

// @expect-no-error glass-lint rule=js:electron.dialog message_id=detected
require("other-electron").dialog.showSaveDialog({});

// A same-shaped local object must not be treated as Electron.
const localElectron = { dialog: { showSaveDialog() {} } };
// @expect-no-error glass-lint rule=js:electron.dialog message_id=detected
localElectron.dialog.showSaveDialog({});
