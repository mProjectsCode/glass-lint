// @case description shadowed, reassigned, dynamic, and lookalike receivers
// @tool glass-lint rules=obsidian:vault.access

// @expect-no-error glass-lint rule=obsidian:vault.access
const localApp = { vault: {} };
localApp.vault;

// @expect-no-error glass-lint rule=obsidian:vault.access
app[dynamicProperty];

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:vault.access
  return app.vault;
}
shadowed({ vault: {} });
