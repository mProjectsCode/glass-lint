// @case description negative fixture for obsidian:vault.config-directory
// @tool glass-lint rules=obsidian:vault.config-directory

// @expect-no-error glass-lint rule=obsidian:vault.config-directory message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const ordinaryConfig = ".config/obsidian";
