// @case description lifecycle lookalikes and invalidated plugin provenance
// @tool glass-lint rules=obsidian:plugins.load-unload
// @expect-no-error glass-lint rule=obsidian:plugins.load-unload
const local = { load() {}, unload() {} };
local.load(); local.unload();
// @expect-no-error glass-lint rule=obsidian:plugins.load-unload
function inspect(app) { app.plugins.loadPlugin("calendar"); }
const plugin = app.plugins.getPlugin("calendar");
plugin = local;
// @expect-no-error glass-lint rule=obsidian:plugins.load-unload
plugin.unload();
const method = methodName;
// @expect-no-error glass-lint rule=obsidian:plugins.load-unload
app.plugins[method]("calendar");
