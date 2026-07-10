// @case description positive fixture for js:electron.module
// @tool glass-lint rules=js:electron.module
// ESM imports are reported at module load time.
// @expect-error glass-lint rule=js:electron.module message_id=detected
import {ipcRenderer} from "electron";
// @expect-error glass-lint rule=js:electron.module message_id=detected
import * as secondElectron from "electron";

// Static CommonJS and interop loads are also module provenance.
// @expect-error glass-lint rule=js:electron.module message_id=detected
const electron = require("electron");
// @expect-error glass-lint rule=js:electron.module message_id=detected
const interopElectron = __toESM(require("electron"));
