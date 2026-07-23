// @case description case-sensitive literal boundary and dynamic values
// @tool glass-lint rules=obsidian:vault.config-directory

// @expect-no-error glass-lint rule=obsidian:vault.config-directory
const ordinaryConfig = ".config/obsidian";
// @expect-no-error glass-lint rule=obsidian:vault.config-directory
const wrongCase = ".Obsidian/plugins/example";
// @expect-no-error glass-lint rule=obsidian:vault.config-directory
const splitMarker = ".obsidian" + "/plugins/example";
// @expect-no-error glass-lint rule=obsidian:vault.config-directory
const dynamicMarker = prefix + "/plugins/example";
// @expect-no-error glass-lint rule=obsidian:vault.config-directory
const staticOtherPath = `config/.obsidianish/${pluginId}`;
