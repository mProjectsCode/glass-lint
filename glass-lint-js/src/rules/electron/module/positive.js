// @case description positive fixture for js:electron.module
// @tool glass-lint rules=js:electron.module
// @expect-error glass-lint rule=js:electron.module message_id=detected
import {ipcRenderer} from "electron";
// second independent example

// @expect-error glass-lint rule=js:electron.module message_id=detected
import * as secondElectron from "electron";
// Migrated: system/node-and-electron-requires.js

// @expect-error glass-lint rule=js:electron.module message_id=detected
const electron = __toESM(require("electron"));
