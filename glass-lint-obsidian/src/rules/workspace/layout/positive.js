// @case description all configured layout calls through rooted aliases and static properties
// @tool glass-lint rules=obsidian:workspace.layout

// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
app.workspace.getLayout();
// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
app.workspace.changeLayout(layout);
// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
app.workspace.requestSaveLayout();

const workspace = this.app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
workspace["changeLayout"](otherLayout);
const workspaceAlias = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
workspaceAlias.requestSaveLayout();
