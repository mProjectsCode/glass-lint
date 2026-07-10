// @case description positive fixture for obsidian:workspace.open
// @tool glass-lint rules=obsidian:workspace.open
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.openLinkText(name, source);
// second independent example
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.openLinkText("second", source);
