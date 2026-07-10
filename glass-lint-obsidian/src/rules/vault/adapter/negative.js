// @case description negative fixture for obsidian:vault.adapter
// @tool glass-lint rules=obsidian:vault.adapter
// @expect-no-error glass-lint rule=obsidian:vault.adapter message_id=detected
function localLookalike() { return null; }
localLookalike();
