// @case description negative fixture for obsidian:vault.write
// @tool glass-lint rules=obsidian:vault.write

// @expect-no-error glass-lint rule=obsidian:vault.write message_id=detected
function localLookalike() { return null; }
localLookalike();

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:vault.write message_id=detected
  app.vault.modify(file, text);
}
shadowed({ vault: { modify() {} } });

function localVaultOnly() {
  const app = { vault: { modify() {} } };
  app.vault.modify(file, text);
}
localVaultOnly();
