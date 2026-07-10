// @case description positive fixture for obsidian:workspace.leaf-management
// @tool glass-lint rules=obsidian:workspace.leaf-management
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.getLeavesOfType('x');
// second independent example
// @expect-error glass-lint rule=obsidian:workspace.leaf-management message_id=detected
app.workspace.revealLeaf(leaf);
