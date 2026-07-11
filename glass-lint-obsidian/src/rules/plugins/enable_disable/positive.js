// @case description changing community plugin enabled state
// @tool glass-lint rules=obsidian:plugins.enable-disable
// @expect-error glass-lint rule=obsidian:plugins.enable-disable message_id=detected
app.plugins.enablePlugin("calendar");
// @expect-error glass-lint rule=obsidian:plugins.enable-disable message_id=detected
this.app.plugins.disablePlugin(pluginId);

const manager = app.plugins;
// @expect-error glass-lint rule=obsidian:plugins.enable-disable message_id=detected
manager["enablePluginAndSave"]("templater-obsidian");
// @expect-error glass-lint rule=obsidian:plugins.enable-disable message_id=detected
manager.disablePluginAndSave("calendar");
