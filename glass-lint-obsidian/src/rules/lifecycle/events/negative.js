// @case description negative fixture for obsidian:lifecycle.events
// @tool glass-lint rules=obsidian:lifecycle.events
// @expect-no-error glass-lint rule=obsidian:lifecycle.events message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this.registerEventual(handler);
