// @case description shadowed, dynamic, unsupported, and reassigned workspace events
// @tool glass-lint rules=obsidian:workspace.events
// @expect-no-error glass-lint rule=obsidian:workspace.events message_id=detected
const localApp = { workspace: { on() {} } };
localApp.workspace.on("layout-change", handler);

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:workspace.events message_id=detected
  app.workspace.on("layout-change", handler);
}
shadowed({ workspace: { on() {} } });

const eventName = getEventName();
// @expect-no-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on(eventName, handler);
// @expect-no-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("unsupported-event", handler);

let workspace = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
workspace.on("quit", handler);
workspace = localWorkspace;
// @expect-no-error glass-lint rule=obsidian:workspace.events message_id=detected
workspace.on("quit", handler);
