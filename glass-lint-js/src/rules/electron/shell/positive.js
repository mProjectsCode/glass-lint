// @case description positive fixture for electron:electron.shell
// @tool glass-lint rules=electron:electron.shell
import * as electron from "electron";

// Proven namespace calls.
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electron.shell.openExternal("https://example.com");
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electron.shell.openPath("/tmp/second");
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electron.shell.showItemInFolder("/tmp/item");
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electron.shell.trashItem("/tmp/trash");
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electron.shell.beep();
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electron.shell.readShortcutLink("/tmp/link");
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electron.shell.writeShortcutLink("/tmp/link", "create", details);

// Namespace aliases retain module provenance.
const electronAlias = electron;
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electronAlias.shell.openExternal("https://alias.example");

// CommonJS and static interop namespace loads are supported.
const electronCjs = require("electron");
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electronCjs.shell.openPath("/tmp/commonjs");
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
require("electron").shell.openExternal("https://direct.example");
const electronInterop = __toESM(require("electron"));
// @expect-error glass-lint rule=electron:electron.shell message_id=detected
electronInterop.shell.openPath("/tmp/interop");
