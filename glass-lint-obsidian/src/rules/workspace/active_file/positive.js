// @case description positive fixture for obsidian:workspace.active-file
// @tool glass-lint rules=obsidian:workspace.active-file

// @expect-error glass-lint rule=obsidian:workspace.active-file message_id=detected
app.workspace.getActiveFile();
const w1 = this.app.workspace;

// @expect-error glass-lint rule=obsidian:workspace.active-file message_id=detected
w1.getActiveFile();
const w2 = app.workspace;

// @expect-error glass-lint rule=obsidian:workspace.active-file message_id=detected
w2.getActiveFile();
// @expect-error glass-lint rule=obsidian:workspace.active-file message_id=detected
app.workspace.getActiveFile();
