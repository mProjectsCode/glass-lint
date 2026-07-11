// @case description case-sensitive literal boundary and dynamic values
// @tool glass-lint rules=obsidian:vault.config-directory

// @expect-no-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const ordinaryConfig = ".config/obsidian";
// @expect-no-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const wrongCase = ".Obsidian/plugins/example";
// @expect-no-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const splitMarker = ".obsidian" + "/plugins/example";
// @expect-no-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const dynamicMarker = prefix + "/plugins/example";
// @expect-no-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const staticOtherPath = `config/.obsidianish/${pluginId}`;
