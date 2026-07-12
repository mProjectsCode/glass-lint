// @case description positive fixture for js:browser.file-dialog
// @tool glass-lint rules=js:browser.file-dialog
const input = document.createElement("input");
input.type = "file";
// @expect-error-after glass-lint rule=js:browser.file-dialog message_id=detected
// Direct aliases retain the tracked input state.
const original = document.createElement("input");
const aliasedInput = original;
aliasedInput.type = "file";
// @expect-error-after glass-lint rule=js:browser.file-dialog message_id=detected

// The flow emits when the file type is configured; opening it is not required.
// @expect-error glass-lint rule=js:browser.file-dialog message_id=detected
const fileInput = document.createElement("input"); fileInput.type = "file";
// @expect-error-after glass-lint rule=js:browser.file-dialog message_id=detected
