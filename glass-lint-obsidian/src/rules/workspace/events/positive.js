// @case description documented workspace event registrations and aliases
// @tool glass-lint rules=obsidian:workspace.events
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("active-leaf-change", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("file-open", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("layout-change", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("window-open", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("window-close", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("quit", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("editor-change", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("editor-paste", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("editor-drop", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("file-menu", handler);
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
app.workspace.on("editor-menu", handler);

const workspace = this.app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.events message_id=detected
workspace["on"]("layout-change", handler);
