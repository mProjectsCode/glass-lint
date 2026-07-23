// @case description generic plugin instance, manifest, and enabled-state access
// @tool glass-lint rules=obsidian:plugins.access
// @expect-error glass-lint rule=obsidian:plugins.access
app.plugins.getPlugin("dataview");
// @expect-error glass-lint rule=obsidian:plugins.access
app.plugins.plugins["calendar"];
// @expect-error glass-lint rule=obsidian:plugins.access
app.plugins.plugins.calendar;
// @expect-error glass-lint rule=obsidian:plugins.access
app.plugins.manifests["templater-obsidian"];
// @expect-error glass-lint rule=obsidian:plugins.access
app.plugins.enabledPlugins.has("calendar");

const manager = this.app.plugins;
// @expect-error glass-lint rule=obsidian:plugins.access
manager["plugins"][pluginId];
