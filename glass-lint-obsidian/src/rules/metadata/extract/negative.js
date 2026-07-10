// @case description negative fixture for obsidian:metadata.extract
// @tool glass-lint rules=obsidian:metadata.extract
// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
function localLookalike() { return null; }
localLookalike();
