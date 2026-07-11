// @case description runtime plugin loading and unloading
// @tool glass-lint rules=obsidian:plugins.load-unload
// @expect-error glass-lint rule=obsidian:plugins.load-unload message_id=detected
app.plugins.loadPlugin("calendar");
// @expect-error glass-lint rule=obsidian:plugins.load-unload message_id=detected
app.plugins.unloadPlugin(pluginId);
// @expect-error glass-lint rule=obsidian:plugins.load-unload message_id=detected
app.plugins.getPlugin("calendar").unload();

const plugin = app.plugins.plugins["calendar"];
// @expect-error glass-lint rule=obsidian:plugins.load-unload message_id=detected
plugin.load();
