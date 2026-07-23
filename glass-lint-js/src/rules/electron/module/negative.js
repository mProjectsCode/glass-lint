// @case description negative fixture for electron:electron.module
// @tool glass-lint rules=electron:electron.module
// Similar module names are not Electron.
// @expect-no-error glass-lint rule=electron:electron.module
import unrelated from "not-electron";
// @expect-no-error glass-lint rule=electron:electron.module
import mainLike from "electron-main-helper";
// @expect-no-error glass-lint rule=electron:electron.module
import rendererLike from "electron/renderer-helper";

// A shadowed CommonJS loader is not a module import.
function shadowed(require) {
  // @expect-no-error glass-lint rule=electron:electron.module
  require("electron");
}
shadowed(() => ({}));

// A local helper with an unrelated name does not establish module provenance.
// @expect-no-error glass-lint rule=electron:electron.module
function localLookalike() { return null; }
localLookalike();

// Same-shaped local exports are not Electron module members.
const localElectron = {
  BrowserWindow: class {},
  webContents: { fromId() {}, fromFrame() {} },
  session: { fromPartition() {} },
  contextBridge: { exposeInMainWorld() {} },
  globalShortcut: { register() {}, unregister() {} },
  desktopCapturer: { getSources() {} },
  nativeTheme: { shouldUseDarkColors: false },
  powerMonitor: { getSystemIdleState() {}, getSystemIdleTime() {} },
  app: { getPath() {}, getVersion() {}, getAppPath() {}, getName() {}, quit() {}, isPackaged: false },
  clipboard: { readText() {}, writeText() {}, readHTML() {} },
  safeStorage: { encryptString() {}, isEncryptionAvailable() {} },
  screen: { getAllDisplays() {}, getCursorScreenPoint() {} },
  protocol: { registerFileProtocol() {}, registerStringProtocol() {} },
  nativeImage: { createFromPath() {}, createFromBuffer() {} },
  BrowserWindow: { getAllWindows() {} },
};
// @expect-no-error glass-lint rule=electron:electron.module
new localElectron.BrowserWindow();
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.webContents.fromId(1);
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.nativeTheme.shouldUseDarkColors;
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.app.getPath("userData");
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.clipboard.readText();
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.safeStorage.encryptString("local");
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.screen.getAllDisplays();
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.app.getAppPath();
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.clipboard.readHTML();
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.screen.getCursorScreenPoint();
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.protocol.registerStringProtocol("local", handler);
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.nativeImage.createFromBuffer(buffer);
// @expect-no-error glass-lint rule=electron:electron.module
localElectron.BrowserWindow.getAllWindows();
