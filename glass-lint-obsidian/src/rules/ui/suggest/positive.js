// @case description positive fixture for obsidian:ui.suggest
// @tool glass-lint rules=obsidian:ui.suggest
// @expect-error glass-lint rule=obsidian:ui.suggest message_id=detected
this.registerEditorSuggest(s);
// second independent example
// @expect-error glass-lint rule=obsidian:ui.suggest message_id=detected
this.registerEditorSuggest(secondSuggest);
