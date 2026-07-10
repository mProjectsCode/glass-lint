// @case description negative fixture for obsidian:vault.resource-url
// @tool glass-lint rules=obsidian:vault.resource-url
// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
function localLookalike() { return null; }
localLookalike();
