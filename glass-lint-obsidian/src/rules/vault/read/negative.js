// @case description negative fixture for obsidian:vault.read
// @tool glass-lint rules=obsidian:vault.read

// @expect-no-error glass-lint rule=obsidian:vault.read message_id=detected
function localLookalike() { return null; }
localLookalike();

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:vault.read message_id=detected
  app.vault.read(file);
}
shadowed({ vault: { read() {} } });

let staleVault = this.app.vault;
staleVault = localStore;
staleVault.read(file);
