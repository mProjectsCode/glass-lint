// @case description Ported old classifier cases: file dialog requires connected file input type flow
// @tool glass-lint rules=obsidian:ui.file_dialog

const input = document.createElement("input"); // @expect-error glass-lint rule=obsidian:ui.file_dialog message_id=detected
input.type = "file";
