// @case description rooted active-editor reads through aliases and static properties
// @tool glass-lint rules=obsidian:workspace.active-editor

// @expect-error glass-lint rule=obsidian:workspace.active-editor message_id=detected
app.workspace.activeEditor;

const workspace = this.app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.active-editor message_id=detected
workspace.activeEditor;
const workspaceAlias = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.active-editor message_id=detected
workspaceAlias["activeEditor"];
