// @case description positive fixture for obsidian:editor.extension
// @tool glass-lint rules=obsidian:editor.extension
// @expect-error glass-lint rule=obsidian:editor.extension message_id=detected
this.registerEditorExtension(ext);
// second independent example
// @expect-error glass-lint rule=obsidian:editor.extension message_id=detected
this.registerEditorExtension(secondExtension);
