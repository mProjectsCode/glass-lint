// @case description shadowed, dynamic, and unsupported file-system calls
// @tool glass-lint rules=browser:browser.filesystem
const window = { showDirectoryPicker() {} };
// @expect-no-error glass-lint rule=browser:browser.filesystem
window.showDirectoryPicker();

const property = getPropertyName();
// @expect-no-error glass-lint rule=browser:browser.filesystem
globalThis.window[property]();
// @expect-no-error glass-lint rule=browser:browser.filesystem
window.showOpenFilePicker();

function localWindow(window) {
    const directory = window.showDirectoryPicker();
    // @expect-no-error glass-lint rule=browser:browser.filesystem
    directory.getFileHandle("local.md");
}
localWindow({ showDirectoryPicker() { return {}; } });
