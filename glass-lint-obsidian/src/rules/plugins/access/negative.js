// @case description plugin-manager shadowing, reassignment, and lookalikes
// @tool glass-lint rules=obsidian:plugins.access
// @expect-no-error glass-lint rule=obsidian:plugins.access message_id=detected
function inspect(app) { return app.plugins.plugins["calendar"]; }

// @expect-no-error glass-lint rule=obsidian:plugins.access message_id=detected
const local = { plugins: { plugins: { calendar: {} } } };
local.plugins.plugins.calendar;

let manager = this.app.plugins;
manager = local.plugins;
// @expect-no-error glass-lint rule=obsidian:plugins.access message_id=detected
manager.plugins.calendar;

// @expect-no-error glass-lint rule=obsidian:plugins.access message_id=detected
const documentation = "dataview and datacore integration";
