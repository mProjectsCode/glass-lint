// @case description negative fixture for obsidian:editor.suggest
// @tool glass-lint rules=obsidian:editor.suggest
// @expect-no-error glass-lint rule=obsidian:editor.suggest message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=obsidian:editor.suggest message_id=detected
this.registerEditorSuggestion(handler);
