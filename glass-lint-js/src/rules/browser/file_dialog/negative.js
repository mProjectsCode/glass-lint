// @case description negative fixture for js:browser.file-dialog
// @tool glass-lint rules=js:browser.file-dialog
// @expect-no-error glass-lint rule=js:browser.file-dialog message_id=detected
document.createElement("div");
const textInput = document.createElement("input");
// @expect-no-error glass-lint rule=js:browser.file-dialog message_id=detected
textInput.type = "text";

// Migrated: interface/text-inputs-ignored.js
const legacyTextInput = document.createElement("input");
legacyTextInput.type = "text";
