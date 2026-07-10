// @case description negative fixture for obsidian:view.register
// @tool glass-lint rules=obsidian:view.register
// @expect-no-error glass-lint rule=obsidian:view.register message_id=detected
function localLookalike() { return null; }
localLookalike();
