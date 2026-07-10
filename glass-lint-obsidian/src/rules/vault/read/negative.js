// @case description negative fixture for obsidian:vault.read
// @tool glass-lint rules=obsidian:vault.read
// @expect-no-error glass-lint rule=obsidian:vault.read message_id=detected
function localLookalike() { return null; }
localLookalike();
