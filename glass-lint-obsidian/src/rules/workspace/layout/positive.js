// @case description all configured layout calls through rooted aliases and static properties
// @tool glass-lint rules=obsidian:workspace.layout

// @expect-error glass-lint rule=obsidian:workspace.layout
app.workspace.getLayout();
// @expect-error glass-lint rule=obsidian:workspace.layout
app.workspace.changeLayout(layout);
// @expect-error glass-lint rule=obsidian:workspace.layout
app.workspace.requestSaveLayout();

const workspace = this.app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.layout
workspace["changeLayout"](otherLayout);
const workspaceAlias = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.layout
workspaceAlias.requestSaveLayout();
