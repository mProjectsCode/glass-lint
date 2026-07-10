// @case description File dialog detection requires connected file-input type flow
// @tool glass-lint rules=js:browser.file-dialog
// @tool eslint-obsidianmd config=recommended

const input = document.createElement("input"); // @expect-error glass-lint rule=js:browser.file-dialog message_id=detected
input.type = "file";
