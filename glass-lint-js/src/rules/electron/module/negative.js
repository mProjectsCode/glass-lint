// @case description negative fixture for electron:electron.module
// @tool glass-lint rules=electron:electron.module
// Similar module names are not Electron.
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
import unrelated from "not-electron";
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
import mainLike from "electron-main-helper";
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
import rendererLike from "electron/renderer-helper";

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
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
new localElectron.BrowserWindow();
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.webContents.fromId(1);
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.nativeTheme.shouldUseDarkColors;
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.app.getPath("userData");
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.clipboard.readText();
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.safeStorage.encryptString("local");
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.screen.getAllDisplays();
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.app.getAppPath();
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.clipboard.readHTML();
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.screen.getCursorScreenPoint();
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.protocol.registerStringProtocol("local", handler);
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.nativeImage.createFromBuffer(buffer);
// @expect-no-error glass-lint rule=electron:electron.module message_id=detected
localElectron.BrowserWindow.getAllWindows();
