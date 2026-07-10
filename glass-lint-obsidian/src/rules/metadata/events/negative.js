// @case description negative fixture for obsidian:metadata.events
// @tool glass-lint rules=obsidian:metadata.events
// @expect-no-error glass-lint rule=obsidian:metadata.events message_id=detected
function localLookalike() { return null; }
localLookalike();
