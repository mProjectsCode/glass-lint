// @case description configured plugin-manager calls and reads
// @tool glass-lint rules=obsidian:plugins.other-access
// @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
app.plugins.getPlugin('x');
// @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
app.plugins.enabledPlugins;
// @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
app.plugins.manifests;

// Rooted aliases and static computed properties retain provenance.
const plugins = this.app.plugins;

// @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
plugins.getPlugin("dataview");
// @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
app['plugins']['enabledPlugins'];
// @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
plugins['manifests'];
