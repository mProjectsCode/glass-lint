// @case description positive fixture for obsidian:plugins.other-access
// @tool glass-lint rules=obsidian:plugins.other-access
// @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
app.plugins.getPlugin('x');
// second independent example
// @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
app.plugins.getPlugin("second");

// Migrated: vault/vault-workspace-metadata-apis.js
const legacyPlugins = this.app.plugins;
legacyPlugins.getPlugin("dataview"); // @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
