// @case description A plugin registers a command
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=recommended
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any

import { Plugin } from "obsidian";

export default class CommandPlugin extends Plugin {
  onload() {
    this.history = [];
    this.registerCommands();
  }

  registerCommands() {
    const commands = [
      ["uppercase-selection", "Uppercase selection", (editor) => this.uppercase(editor)],
      ["lowercase-selection", "Lowercase selection", (editor) => this.lowercase(editor)],
      ["repeat-transform", "Repeat last transform", (editor) => this.repeat(editor)],
    ];
    for (const [id, name, editorCallback] of commands) {
      this.addCommand({ id, name, editorCallback });
    }
  }

  uppercase(editor) {
    this.transform(editor, "uppercase", (text) => text.toUpperCase());
  }

  lowercase(editor) {
    this.transform(editor, "lowercase", (text) => text.toLowerCase());
  }

  transform(editor, name, operation) {
    const source = editor.getSelection();
    if (!source) return;
    const result = operation(source);
    editor.replaceSelection(result);
    this.history.push({ name, source, result });
    if (this.history.length > 10) this.history.shift();
  }

  repeat(editor) {
    const previous = this.history.at(-1);
    if (!previous) return;
    if (previous.name === "uppercase") this.uppercase(editor);
    if (previous.name === "lowercase") this.lowercase(editor);
  }

  onunload() {
    this.history.length = 0;
  }
}
