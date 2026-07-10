// @case description positive fixture for obsidian:workspace.layout
// @tool glass-lint rules=obsidian:workspace.layout

// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
app.workspace.getLayout();
const w1 = this.app.workspace;

// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
w1.requestSaveLayout();
const w2 = app.workspace;

// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
w2.changeLayout({});
// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
app.workspace.getLayout();
