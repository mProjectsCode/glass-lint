// @case description positive fixture for obsidian:vault.config-directory
// @tool glass-lint rules=obsidian:vault.config-directory

// @expect-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const p='.obsidian/';
// @expect-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const secondConfig = ".obsidian/plugins/second";
// @expect-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const pluginConfig = ".obsidian/plugins/example/data.json";
