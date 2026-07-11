// @case description plugin lifecycle lookalikes and dynamic methods
// @tool glass-lint rules=obsidian:plugins.enable-disable
// @expect-no-error glass-lint rule=obsidian:plugins.enable-disable message_id=detected
function inspect(app) { app.plugins.enablePlugin("calendar"); }
const local = { enablePlugin() {}, disablePluginAndSave() {} };
// @expect-no-error glass-lint rule=obsidian:plugins.enable-disable message_id=detected
local.enablePlugin("calendar");
const method = methodName;
// @expect-no-error glass-lint rule=obsidian:plugins.enable-disable message_id=detected
app.plugins[method]("calendar");
// @expect-no-error glass-lint rule=obsidian:plugins.enable-disable message_id=detected
app.plugins.enablePlugins("calendar");
// @expect-no-error glass-lint rule=obsidian:plugins.enable-disable message_id=detected
app.plugins.disablePlugins("calendar");
