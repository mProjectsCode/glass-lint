// @case description negative fixture for obsidian:vault.delete
// @tool glass-lint rules=obsidian:vault.delete

// @expect-no-error glass-lint rule=obsidian:vault.delete message_id=detected
function localLookalike() { return null; }
localLookalike();
function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:vault.delete message_id=detected
  app.vault.delete(file);
}
shadowed({ vault: { delete() {} } });
