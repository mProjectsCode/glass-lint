// @case description File dialog detection requires connected file-input type flow
// @tool glass-lint rules=obsidian:ui.file_dialog
// @tool eslint-obsidianmd config=recommended

const input = document.createElement("input"); // @expect-error glass-lint rule=obsidian:ui.file_dialog message_id=detected
input.type = "file";
