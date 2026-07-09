// @case description Text inputs do not report a file dialog
// @tool glass-lint rules=obsidian:ui.file_dialog

const input = document.createElement("input");
input.type = "text";
