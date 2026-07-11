// @case description shadowed, reassigned, dynamic, and lookalike receivers
// @tool glass-lint rules=obsidian:vault.access

// @expect-no-error glass-lint rule=obsidian:vault.access message_id=detected
const localApp = { vault: {} };
localApp.vault;

// @expect-no-error glass-lint rule=obsidian:vault.access message_id=detected
app[dynamicProperty];

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:vault.access message_id=detected
  return app.vault;
}
shadowed({ vault: {} });
