// @case description negative fixture for obsidian:workspace.open
// @tool glass-lint rules=obsidian:workspace.open
// @expect-no-error glass-lint rule=obsidian:workspace.open message_id=detected
function localLookalike() { return null; }
localLookalike();
