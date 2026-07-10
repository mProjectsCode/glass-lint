// @case description positive fixture for js:browser.file-dialog
// @tool glass-lint rules=js:browser.file-dialog
// @expect-error glass-lint rule=js:browser.file-dialog message_id=detected
const input = document.createElement("input"); input.type = "file"; input.click();
// second independent example
// @expect-error glass-lint rule=js:browser.file-dialog message_id=detected
const secondInput = document.createElement("input"); secondInput.type = "file";
