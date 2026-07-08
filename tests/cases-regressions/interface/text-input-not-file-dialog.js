// @case description Ported old classifier regression: text input should not be a file dialog
// @tool glass-lint rules=obsidian:ui.file_dialog

const input = document.createElement("input");
input.type = "text";
