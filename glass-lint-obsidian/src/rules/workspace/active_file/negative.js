// @case description local lookalikes, shadowing, dynamic and unlisted calls, and reassignment
// @tool glass-lint rules=obsidian:workspace.active-file

const localApp = { workspace: { getActiveFile() {} } };
// @expect-no-error glass-lint rule=obsidian:workspace.active-file message_id=detected
localApp.workspace.getActiveFile();

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:workspace.active-file message_id=detected
  app.workspace.getActiveFile();
}
shadowed({ workspace: { getActiveFile() {} } });

// Dynamic and unlisted methods are outside the configured call.
// @expect-no-error glass-lint rule=obsidian:workspace.active-file message_id=detected
app.workspace[method]();
// @expect-no-error glass-lint rule=obsidian:workspace.active-file message_id=detected
app.workspace.getActiveEditor();

let workspace = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.active-file message_id=detected
workspace.getActiveFile();
workspace = localWorkspace;
// @expect-no-error-after glass-lint rule=obsidian:workspace.active-file message_id=detected
workspace.getActiveFile();
