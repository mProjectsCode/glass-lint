// @case description negative fixture for obsidian:workspace.active-editor
// @tool glass-lint rules=obsidian:workspace.active-editor

// @expect-no-error glass-lint rule=obsidian:workspace.active-editor message_id=detected
function localLookalike() { return null; }
localLookalike();
function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:workspace.active-editor message_id=detected
  return app.workspace.activeEditor;
}
shadowed({ workspace: {} });
