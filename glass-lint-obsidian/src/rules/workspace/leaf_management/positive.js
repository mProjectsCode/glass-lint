// @case description all configured leaf calls through rooted aliases and static properties
// @tool glass-lint rules=obsidian:workspace.leaf-management

// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.getLeavesOfType("view");
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.detachLeavesOfType("view");
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.revealLeaf(leaf);

const workspace = this.app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
workspace["getLeavesOfType"]("computed");
const workspaceAlias = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
workspaceAlias.detachLeavesOfType("alias");
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.getLeaf(true);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.getLeafById("leaf");
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.getLeftLeaf(leaf);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.getRightLeaf(leaf);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.ensureSideLeaf("left");
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.iterateRootLeaves(callback);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.iterateAllLeaves(callback);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.setActiveLeaf(leaf);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.moveLeafToPopout(leaf);
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.openPopoutLeaf();
