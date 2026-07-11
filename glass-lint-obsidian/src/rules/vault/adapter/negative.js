// @case description shadowed, reassigned, dynamic, and lookalike receivers
// @tool glass-lint rules=obsidian:vault.adapter

// @expect-no-error glass-lint rule=obsidian:vault.adapter message_id=detected
const localApp = { vault: { adapter: {} } };
localApp.vault.adapter;

// @expect-no-error glass-lint rule=obsidian:vault.adapter message_id=detected
app.vault[dynamicProperty];

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:vault.adapter message_id=detected
  return app.vault.adapter;
}
shadowed({ vault: {} });
