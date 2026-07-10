// @case description positive fixture for js:electron.shell
// @tool glass-lint rules=js:electron.shell
import * as electron from "electron";

// @expect-error glass-lint rule=js:electron.shell message_id=detected
electron.shell.openExternal("https://example.com");
// second independent example

// @expect-error glass-lint rule=js:electron.shell message_id=detected
electron.shell.openPath("/tmp/second");
