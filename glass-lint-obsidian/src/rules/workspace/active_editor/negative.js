// @case description local lookalikes, shadowing, dynamic and unlisted reads, and reassignment
// @tool glass-lint rules=obsidian:workspace.active-editor

const localApp = { workspace: { activeEditor: null } };
// @expect-no-error glass-lint rule=obsidian:workspace.active-editor
localApp.workspace.activeEditor;

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:workspace.active-editor
  return app.workspace.activeEditor;
}
shadowed({ workspace: { activeEditor: null } });

// Dynamic and unlisted properties are outside the configured read.
// @expect-no-error glass-lint rule=obsidian:workspace.active-editor
app.workspace[member];
// @expect-no-error glass-lint rule=obsidian:workspace.active-editor
app.workspace.activeFile;

let workspace = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.active-editor
workspace.activeEditor;
workspace = localWorkspace;
// @expect-no-error-after glass-lint rule=obsidian:workspace.active-editor
workspace.activeEditor;
