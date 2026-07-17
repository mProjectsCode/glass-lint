// @case description negative fixture for electron:electron.module
// @tool glass-lint rules=electron:electron.module
// Similar module names are not Electron.
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
import unrelated from "not-electron";

// A shadowed CommonJS loader is not a module import.
function shadowed(require) {
  // @expect-no-error glass-lint rule=electron:electron.module message_id=detected
  require("electron");
}
shadowed(() => ({}));

// A local helper with an unrelated name does not establish module provenance.
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
function localLookalike() { return null; }
localLookalike();
