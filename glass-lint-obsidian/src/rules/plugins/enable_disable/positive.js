// @case description changing community plugin enabled state
// @tool glass-lint rules=obsidian:plugins.enable-disable
// @expect-error glass-lint rule=obsidian:plugins.enable-disable
app.plugins.enablePlugin("calendar");
// @expect-error glass-lint rule=obsidian:plugins.enable-disable
this.app.plugins.disablePlugin(pluginId);

const manager = app.plugins;
// @expect-error glass-lint rule=obsidian:plugins.enable-disable
manager["enablePluginAndSave"]("templater-obsidian");
// @expect-error glass-lint rule=obsidian:plugins.enable-disable
manager.disablePluginAndSave("calendar");
