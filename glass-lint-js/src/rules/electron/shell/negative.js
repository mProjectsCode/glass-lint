// @case description negative fixture for electron:electron.shell
// @tool glass-lint rules=electron:electron.shell
// Same-shaped local objects are not Electron.
const electron = { shell: { openExternal() {} } };
// @expect-no-error glass-lint rule=electron:electron.shell
electron.shell.openExternal("local");

// Reassignment drops module provenance from an alias.
let reassigned = require("electron");
reassigned = {};
// @expect-no-error glass-lint rule=electron:electron.shell
reassigned.shell.openPath("/tmp/reassigned");

// Shadowed require prevents a local object from becoming a module alias.
function shadowed(require) {
  const localElectron = require("electron");
  // @expect-no-error glass-lint rule=electron:electron.shell
  localElectron.shell.openExternal("local");
}
shadowed(() => ({}));

// Inline CommonJS member chains share module provenance.
// @expect-error glass-lint rule=electron:electron.shell
require("electron").shell.openExternal("inline");

// A same-named helper is unrelated to Electron shell APIs.
function localLookalike() { return null; }
// @expect-no-error glass-lint rule=electron:electron.shell
localLookalike();
