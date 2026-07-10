// @case description negative fixture for obsidian:workspace.active-file
// @tool glass-lint rules=obsidian:workspace.active-file

// @expect-no-error glass-lint rule=obsidian:workspace.active-file message_id=detected
function localLookalike() { return null; }
localLookalike();

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:workspace.active-file message_id=detected
  app.workspace.getActiveFile();
}
shadowed({ workspace: { getActiveFile() {} } });
