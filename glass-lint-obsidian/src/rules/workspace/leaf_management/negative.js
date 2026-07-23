// @case description local lookalikes, shadowing, dynamic and unlisted calls, and reassignment
// @tool glass-lint rules=obsidian:workspace.leaf-management

const localApp = { workspace: { revealLeaf() {} } };
// @expect-no-error glass-lint rule=obsidian:workspace.leaf-management
localApp.workspace.revealLeaf(leaf);

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:workspace.leaf-management
  app.workspace.revealLeaf(leaf);
}
shadowed({ workspace: { revealLeaf() {} } });

// Dynamic and unlisted methods are outside the configured calls.
// @expect-no-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace[method](leaf);
// @expect-no-error glass-lint rule=obsidian:workspace.leaf-management
app.workspace.openLinkText(name, source);

let workspace = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.leaf-management
workspace.revealLeaf(leaf);
workspace = localWorkspace;
// @expect-no-error-after glass-lint rule=obsidian:workspace.leaf-management
workspace.revealLeaf(leaf);
