// @case description negative fixture for browser:browser.file-dialog
// @tool glass-lint rules=browser:browser.file-dialog
// @expect-no-error glass-lint rule=browser:browser.file-dialog
document.createElement("div");
const textInput = document.createElement("input");
// @expect-no-error glass-lint rule=browser:browser.file-dialog
textInput.type = "text";

// Static computed property names are normalized as direct writes.
const computedInput = document.createElement("input");
computedInput["type"] = "file";
// @expect-error-after glass-lint rule=browser:browser.file-dialog

// Dynamic attribute values and local lookalikes are not configuration evidence.
const dynamicAttributeInput = document.createElement("input");
dynamicAttributeInput.setAttribute("type", kind);
const localAttributeInput = { setAttribute() {} };
// @expect-no-error glass-lint rule=browser:browser.file-dialog
localAttributeInput.setAttribute("type", "file");

// Reassigning the variable clears its previously tracked source state.
let replacedInput = document.createElement("input");
replacedInput = document.createElement("div");
// @expect-no-error glass-lint rule=browser:browser.file-dialog
replacedInput.type = "file";
