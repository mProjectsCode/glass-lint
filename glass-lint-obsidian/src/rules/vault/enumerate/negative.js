// @case description negative fixture for obsidian:vault.enumerate
// @tool glass-lint rules=obsidian:vault.enumerate
// @expect-no-error glass-lint rule=obsidian:vault.enumerate message_id=detected
function localLookalike() { return null; }
localLookalike();
