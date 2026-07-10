// @case description negative fixture for obsidian:vault.move-copy
// @tool glass-lint rules=obsidian:vault.move-copy
// @expect-no-error glass-lint rule=obsidian:vault.move-copy message_id=detected
function localLookalike() { return null; }
localLookalike();
