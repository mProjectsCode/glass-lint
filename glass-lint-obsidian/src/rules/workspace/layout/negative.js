// @case description negative fixture for obsidian:workspace.layout
// @tool glass-lint rules=obsidian:workspace.layout
// @expect-no-error glass-lint rule=obsidian:workspace.layout message_id=detected
function localLookalike() { return null; }
localLookalike();
