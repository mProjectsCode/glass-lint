// @case description negative fixture for obsidian:vault.events
// @tool glass-lint rules=obsidian:vault.events

// @expect-no-error glass-lint rule=obsidian:vault.events message_id=detected
function localLookalike() { return null; }
localLookalike();
function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:vault.events message_id=detected
  app.vault.on("changed", handler);
}
shadowed({ vault: { on() {} } });
