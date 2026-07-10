// @case description positive fixture for js:electron.shell
// @tool glass-lint rules=js:electron.shell
import * as electron from "electron";

// Proven namespace calls.
// @expect-error glass-lint rule=js:electron.shell message_id=detected
electron.shell.openExternal("https://example.com");
// @expect-error glass-lint rule=js:electron.shell message_id=detected
electron.shell.openPath("/tmp/second");

// Namespace aliases retain module provenance.
const electronAlias = electron;
// @expect-error glass-lint rule=js:electron.shell message_id=detected
electronAlias.shell.openExternal("https://alias.example");

// CommonJS and static interop namespace loads are supported.
const electronCjs = require("electron");
// @expect-error glass-lint rule=js:electron.shell message_id=detected
electronCjs.shell.openPath("/tmp/commonjs");
require("electron").shell.openExternal("https://direct.example");
const electronInterop = __toESM(require("electron"));
// @expect-error glass-lint rule=js:electron.shell message_id=detected
electronInterop.shell.openPath("/tmp/interop");
