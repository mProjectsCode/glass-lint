// @case description positive fixture for obsidian:workspace.layout
// @tool glass-lint rules=obsidian:workspace.layout
// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
app.workspace.getLayout();
// second independent example
// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
app.workspace.getLayout();
