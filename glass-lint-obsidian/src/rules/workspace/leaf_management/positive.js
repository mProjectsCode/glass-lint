// @case description all configured leaf calls through rooted aliases and static properties
// @tool glass-lint rules=obsidian:workspace.leaf-management

// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.getLeavesOfType("view");
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.detachLeavesOfType("view");
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.revealLeaf(leaf);

const workspace = this.app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
workspace["getLeavesOfType"]("computed");
const workspaceAlias = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
workspaceAlias.detachLeavesOfType("alias");
