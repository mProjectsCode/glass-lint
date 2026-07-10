// @case description positive fixture for obsidian:workspace.active-file
// @tool glass-lint rules=obsidian:workspace.active-file
// @expect-error glass-lint rule=obsidian:workspace.active-file message_id=detected
app.workspace.getActiveFile();
// second independent example
// @expect-error glass-lint rule=obsidian:workspace.active-file message_id=detected
app.workspace.getActiveFile();
