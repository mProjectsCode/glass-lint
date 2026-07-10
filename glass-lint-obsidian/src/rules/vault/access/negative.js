// @case description negative fixture for obsidian:vault.access
// @tool glass-lint rules=obsidian:vault.access
// @expect-no-error glass-lint rule=obsidian:vault.access message_id=detected
function localLookalike() { return null; }
localLookalike();
