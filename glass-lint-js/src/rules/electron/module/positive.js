// @case description positive fixture for electron:electron.module
// @tool glass-lint rules=electron:electron.module
// ESM imports are reported at module load time.
// @expect-error glass-lint rule=electron:electron.module message_id=detected
import {ipcRenderer} from "electron";
// @expect-error glass-lint rule=electron:electron.module message_id=detected
import * as secondElectron from "electron";

// Static CommonJS and interop loads are also module provenance.
// @expect-error glass-lint rule=electron:electron.module message_id=detected
const electron = require("electron");
// @expect-error glass-lint rule=electron:electron.module message_id=detected
const interopElectron = __toESM(require("electron"));
// Exact Electron subpath imports retain module-boundary provenance.
// @expect-error glass-lint rule=electron:electron.module message_id=detected
import mainElectron from "electron/main";
// @expect-error glass-lint rule=electron:electron.module message_id=detected
import rendererElectron from "electron/renderer";
// @expect-error glass-lint rule=electron:electron.module message_id=detected
import commonElectron from "electron/common";
// @expect-error glass-lint rule=electron:electron.module message_id=detected
import utilityElectron from "electron/utility";
// @expect-error glass-lint rule=electron:electron.module message_id=detected
import sandboxElectron from "electron/sandbox";

// High-impact Electron exports retain exact module provenance.
// @expect-error glass-lint rule=electron:electron.module message_id=detected
new secondElectron.BrowserWindow({});
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.webContents.fromId(1);
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.webContents.fromFrame(frame);
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.session.fromPartition("persist:demo");
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.contextBridge.exposeInMainWorld("api", api);
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.globalShortcut.register("CommandOrControl+X", handler);
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.globalShortcut.unregister("CommandOrControl+X");
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.desktopCapturer.getSources({ types: ["window"] });
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.nativeTheme.shouldUseDarkColors;
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.powerMonitor.getSystemIdleState(60);
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.powerMonitor.getSystemIdleTime();
// Additional high-impact module exports retain exact provenance.
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.app.getPath("userData");
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.app.getVersion();
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.app.getAppPath();
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.app.getName();
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.app.getLocale();
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.app.isPackaged;
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.app.quit();
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.clipboard.readText();
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.clipboard.writeText("copied");
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.safeStorage.encryptString("secret");
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.safeStorage.isEncryptionAvailable();
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.screen.getAllDisplays();
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.screen.getCursorScreenPoint();
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.protocol.registerFileProtocol("app", handler);
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.protocol.registerStringProtocol("app", handler);
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.nativeImage.createFromPath("icon.png");
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.nativeImage.createFromBuffer(buffer);
// @expect-error glass-lint rule=electron:electron.module message_id=detected
secondElectron.BrowserWindow.getAllWindows();
