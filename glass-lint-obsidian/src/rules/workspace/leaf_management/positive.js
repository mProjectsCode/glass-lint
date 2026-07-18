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
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.getLeaf(true);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.getLeafById("leaf");
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.getLeftLeaf(leaf);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.getRightLeaf(leaf);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.ensureSideLeaf("left");
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.iterateRootLeaves(callback);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.iterateAllLeaves(callback);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.setActiveLeaf(leaf);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.moveLeafToPopout(leaf);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.openPopoutLeaf();
