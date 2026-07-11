// @case description negative receiver, alias, dynamic, and lookalike forms
// @tool glass-lint rules=obsidian:ui.suggest
// @expect-no-error glass-lint rule=obsidian:ui.suggest message_id=detected
plugin.registerEditorSuggest(s);

const registerEditorSuggest = this.registerEditorSuggest;
// @expect-no-error glass-lint rule=obsidian:ui.suggest message_id=detected
registerEditorSuggest(s);

// @expect-no-error glass-lint rule=obsidian:ui.suggest message_id=detected
this[dynamicProperty](handler);

// @expect-no-error glass-lint rule=obsidian:ui.suggest message_id=detected
this.registerEditorSuggests(handler);
