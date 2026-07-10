// @case description positive fixture for obsidian:editor.suggest
// @tool glass-lint rules=obsidian:editor.suggest
// @expect-error glass-lint rule=obsidian:editor.suggest message_id=detected
this.registerEditorSuggest(s);
// second independent example
// @expect-error glass-lint rule=obsidian:editor.suggest message_id=detected
this.registerEditorSuggest(secondSuggest);
