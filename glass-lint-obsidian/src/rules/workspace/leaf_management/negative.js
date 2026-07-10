// @case description negative fixture for obsidian:workspace.leaf-management
// @tool glass-lint rules=obsidian:workspace.leaf-management
// @expect-no-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
function localLookalike() { return null; }
localLookalike();
