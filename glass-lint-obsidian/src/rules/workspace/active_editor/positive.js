// @case description positive fixture for obsidian:workspace.active-editor
// @tool glass-lint rules=obsidian:workspace.active-editor

// @expect-error glass-lint rule=obsidian:workspace.active-editor message_id=detected
app.workspace.activeEditor;
