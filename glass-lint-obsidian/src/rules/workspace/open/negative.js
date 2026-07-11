// @case description local lookalikes, shadowing, dynamic and unlisted calls, and reassignment
// @tool glass-lint rules=obsidian:workspace.open

const localApp = { workspace: { openLinkText() {} } };
// @expect-no-error glass-lint rule=obsidian:workspace.open message_id=detected
localApp.workspace.openLinkText(name, source);

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:workspace.open message_id=detected
  app.workspace.openLinkText(name, source);
}
shadowed({ workspace: { openLinkText() {} } });

// Dynamic and unlisted methods are outside the configured calls.
// @expect-no-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace[method](name, source);
// @expect-no-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.openFile(file);
// The rooted matcher does not follow the intermediate getLeaf() call.
// @expect-no-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.getLeaf().openFile(file);

let workspace = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
workspace.openLinkText(name, source);
workspace = localWorkspace;
// @expect-no-error-after glass-lint rule=obsidian:workspace.open message_id=detected
workspace.openLinkText(name, source);
