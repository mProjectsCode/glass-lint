// @case description negative fixture for obsidian:metadata.traversal
// @tool glass-lint rules=obsidian:metadata.traversal
// @expect-no-error glass-lint rule=obsidian:metadata.traversal message_id=detected
function localLookalike() { return null; }
localLookalike();
