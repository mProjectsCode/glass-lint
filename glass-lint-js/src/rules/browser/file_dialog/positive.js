// @case description positive fixture for browser:browser.file-dialog
// @tool glass-lint rules=browser:browser.file-dialog
const input = document.createElement("input");
input.type = "file";
// @expect-error-after glass-lint rule=browser:browser.file-dialog
// Direct aliases retain the tracked input state.
const original = document.createElement("input");
const aliasedInput = original;
aliasedInput.type = "file";
// @expect-error-after glass-lint rule=browser:browser.file-dialog

// The flow emits when the file type is configured; opening it is not required.
// @expect-error glass-lint rule=browser:browser.file-dialog
const fileInput = document.createElement("input"); fileInput.type = "file";
// @expect-error glass-lint rule=browser:browser.file-dialog
window.showOpenFilePicker();
// @expect-error glass-lint rule=browser:browser.file-dialog
window.showSaveFilePicker();
// @expect-error glass-lint rule=browser:browser.file-dialog
globalThis.showOpenFilePicker();
// @expect-error-after glass-lint rule=browser:browser.file-dialog

// Static setAttribute configuration is equivalent to a direct type write.
const attributeInput = document.createElement("input");
attributeInput.setAttribute("type", "file");
// @expect-error-after glass-lint rule=browser:browser.file-dialog
