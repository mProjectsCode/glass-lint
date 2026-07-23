// @case description plugin-manager shadowing, reassignment, and lookalikes
// @tool glass-lint rules=obsidian:plugins.access
// @expect-no-error glass-lint rule=obsidian:plugins.access
function inspect(app) { return app.plugins.plugins["calendar"]; }

// @expect-no-error glass-lint rule=obsidian:plugins.access
const local = { plugins: { plugins: { calendar: {} } } };
local.plugins.plugins.calendar;

let manager = this.app.plugins;
manager = local.plugins;
// @expect-no-error glass-lint rule=obsidian:plugins.access
manager.plugins.calendar;

// @expect-no-error glass-lint rule=obsidian:plugins.access
const documentation = "dataview and datacore integration";
