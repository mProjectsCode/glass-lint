// @case description positive fixture for js:electron.ipc
// @tool glass-lint rules=js:electron.ipc
import * as electron from "electron";

// @expect-error glass-lint rule=js:electron.ipc message_id=detected
electron.ipcRenderer.send("x");
// second independent example

// @expect-error glass-lint rule=js:electron.ipc message_id=detected
electron.ipcRenderer.invoke("second");
