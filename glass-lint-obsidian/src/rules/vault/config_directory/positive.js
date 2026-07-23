// @case description configured separators, substrings, and static templates
// @tool glass-lint rules=obsidian:vault.config-directory

// @expect-error glass-lint rule=obsidian:vault.config-directory
const forward = '.obsidian/';
// @expect-error glass-lint rule=obsidian:vault.config-directory
const nested = ".obsidian/plugins/second";
// @expect-error glass-lint rule=obsidian:vault.config-directory
const prefixed = "/vault/.obsidian/plugins/example/data.json";
// @expect-error glass-lint rule=obsidian:vault.config-directory
const windows = "C:\\vault\\.obsidian\\plugins\\example";
// @expect-error glass-lint rule=obsidian:vault.config-directory
const staticTemplate = `.obsidian/plugins/${pluginId}`;
// @expect-error glass-lint rule=obsidian:vault.config-directory
app.vault.configDir;
