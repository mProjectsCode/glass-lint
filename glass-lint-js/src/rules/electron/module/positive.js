// @case description positive fixture for electron:electron.module
// @tool glass-lint rules=electron:electron.module
// ESM imports are reported at module load time.
// @expect-error glass-lint rule=electron:electron.module
import {ipcRenderer} from "electron";
// @expect-error glass-lint rule=electron:electron.module
import * as secondElectron from "electron";

// Static CommonJS and interop loads are also module provenance.
// @expect-error glass-lint rule=electron:electron.module
const electron = require("electron");
// @expect-error glass-lint rule=electron:electron.module
const interopElectron = __toESM(require("electron"));
// Exact Electron subpath imports retain module-boundary provenance.
// @expect-error glass-lint rule=electron:electron.module
import mainElectron from "electron/main";
// @expect-error glass-lint rule=electron:electron.module
import rendererElectron from "electron/renderer";
// @expect-error glass-lint rule=electron:electron.module
import commonElectron from "electron/common";
// @expect-error glass-lint rule=electron:electron.module
import utilityElectron from "electron/utility";
// @expect-error glass-lint rule=electron:electron.module
import sandboxElectron from "electron/sandbox";

// High-impact Electron exports retain exact module provenance.
// @expect-error glass-lint rule=electron:electron.module
new secondElectron.BrowserWindow({});
// @expect-error glass-lint rule=electron:electron.module
secondElectron.webContents.fromId(1);
// @expect-error glass-lint rule=electron:electron.module
secondElectron.webContents.fromFrame(frame);
// @expect-error glass-lint rule=electron:electron.module
secondElectron.session.fromPartition("persist:demo");
// @expect-error glass-lint rule=electron:electron.module
secondElectron.contextBridge.exposeInMainWorld("api", api);
// @expect-error glass-lint rule=electron:electron.module
secondElectron.globalShortcut.register("CommandOrControl+X", handler);
// @expect-error glass-lint rule=electron:electron.module
secondElectron.globalShortcut.unregister("CommandOrControl+X");
// @expect-error glass-lint rule=electron:electron.module
secondElectron.desktopCapturer.getSources({ types: ["window"] });
// @expect-error glass-lint rule=electron:electron.module
secondElectron.nativeTheme.shouldUseDarkColors;
// @expect-error glass-lint rule=electron:electron.module
secondElectron.powerMonitor.getSystemIdleState(60);
// @expect-error glass-lint rule=electron:electron.module
secondElectron.powerMonitor.getSystemIdleTime();
// Additional high-impact module exports retain exact provenance.
// @expect-error glass-lint rule=electron:electron.module
secondElectron.app.getPath("userData");
// @expect-error glass-lint rule=electron:electron.module
secondElectron.app.getVersion();
// @expect-error glass-lint rule=electron:electron.module
secondElectron.app.getAppPath();
// @expect-error glass-lint rule=electron:electron.module
secondElectron.app.getName();
// @expect-error glass-lint rule=electron:electron.module
secondElectron.app.getLocale();
// @expect-error glass-lint rule=electron:electron.module
secondElectron.app.isPackaged;
// @expect-error glass-lint rule=electron:electron.module
secondElectron.app.quit();
// @expect-error glass-lint rule=electron:electron.module
secondElectron.clipboard.readText();
// @expect-error glass-lint rule=electron:electron.module
secondElectron.clipboard.writeText("copied");
// @expect-error glass-lint rule=electron:electron.module
secondElectron.safeStorage.encryptString("secret");
// @expect-error glass-lint rule=electron:electron.module
secondElectron.safeStorage.isEncryptionAvailable();
// @expect-error glass-lint rule=electron:electron.module
secondElectron.screen.getAllDisplays();
// @expect-error glass-lint rule=electron:electron.module
secondElectron.screen.getCursorScreenPoint();
// @expect-error glass-lint rule=electron:electron.module
secondElectron.protocol.registerFileProtocol("app", handler);
// @expect-error glass-lint rule=electron:electron.module
secondElectron.protocol.registerStringProtocol("app", handler);
// @expect-error glass-lint rule=electron:electron.module
secondElectron.nativeImage.createFromPath("icon.png");
// @expect-error glass-lint rule=electron:electron.module
secondElectron.nativeImage.createFromBuffer(buffer);
// @expect-error glass-lint rule=electron:electron.module
secondElectron.BrowserWindow.getAllWindows();
