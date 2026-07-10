// @case description Text inputs do not report a file dialog
// @tool glass-lint rules=js:browser.file-dialog
// @tool eslint-obsidianmd config=recommended

const input = document.createElement("input");
input.type = "text";
