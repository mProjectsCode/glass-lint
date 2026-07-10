// @case description negative fixture for obsidian:ui.suggest
// @tool glass-lint rules=obsidian:ui.suggest
// @expect-no-error glass-lint rule=obsidian:ui.suggest message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:ui.suggest message_id=detected
this.registerSuggest(handler);
