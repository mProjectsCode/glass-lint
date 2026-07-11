// @case description local lookalikes, shadowing, dynamic and unlisted calls, and reassignment
// @tool glass-lint rules=obsidian:workspace.layout

const localApp = { workspace: { getLayout() {} } };
// @expect-no-error glass-lint rule=obsidian:workspace.layout message_id=detected
localApp.workspace.getLayout();

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:workspace.layout message_id=detected
  app.workspace.getLayout();
}
shadowed({ workspace: { getLayout() {} } });

// Dynamic and unlisted methods are outside the configured calls.
// @expect-no-error glass-lint rule=obsidian:workspace.layout message_id=detected
app.workspace[method]();
// @expect-no-error glass-lint rule=obsidian:workspace.layout message_id=detected
app.workspace.getLayoutSnapshot();

let workspace = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
workspace.getLayout();
workspace = localWorkspace;
// @expect-no-error-after glass-lint rule=obsidian:workspace.layout message_id=detected
workspace.getLayout();
