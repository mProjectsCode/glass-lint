// @case description shadowed, reassigned, dynamic, and lookalike receivers
// @tool glass-lint rules=obsidian:vault.adapter

// @expect-no-error glass-lint rule=obsidian:vault.adapter
const localApp = { vault: { adapter: {} } };
localApp.vault.adapter;

// @expect-no-error glass-lint rule=obsidian:vault.adapter
app.vault[dynamicProperty];

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:vault.adapter
  return app.vault.adapter;
}
shadowed({ vault: {} });
