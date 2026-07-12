// @case description negative fixture for js:browser.file-dialog
// @tool glass-lint rules=js:browser.file-dialog
// @expect-no-error glass-lint rule=js:browser.file-dialog message_id=detected
document.createElement("div");
const textInput = document.createElement("input");
// @expect-no-error glass-lint rule=js:browser.file-dialog message_id=detected
textInput.type = "text";

// Static computed property names are normalized as direct writes.
const computedInput = document.createElement("input");
computedInput["type"] = "file";
// @expect-error-after glass-lint rule=js:browser.file-dialog message_id=detected

// setAttribute is intentionally not configuration evidence.
const attributeInput = document.createElement("input");
// @expect-no-error glass-lint rule=js:browser.file-dialog message_id=detected
attributeInput.setAttribute("type", "file");

// Reassigning the variable clears its previously tracked source state.
let replacedInput = document.createElement("input");
replacedInput = document.createElement("div");
// @expect-no-error glass-lint rule=js:browser.file-dialog message_id=detected
replacedInput.type = "file";
