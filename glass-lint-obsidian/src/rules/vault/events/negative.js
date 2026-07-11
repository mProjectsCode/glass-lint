// @case description shadowing, reassignment, dynamic, and other event methods
// @tool glass-lint rules=obsidian:vault.events

// @expect-no-error glass-lint rule=obsidian:vault.events message_id=detected
const localApp = { vault: { on() {} } };
localApp.vault.on("changed", handler);
// @expect-no-error glass-lint rule=obsidian:vault.events message_id=detected
function shadowed(app) {
  app.vault.on("changed", handler);
}
shadowed({ vault: { on() {} } });
// @expect-no-error glass-lint rule=obsidian:vault.events message_id=detected
app.vault[eventMethod]("changed", handler);
// @expect-no-error glass-lint rule=obsidian:vault.events message_id=detected
app.vault.off("changed", handler);

let vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.events message_id=detected
vault.on("changed", handler);
vault = localVault;
// @expect-no-error glass-lint rule=obsidian:vault.events message_id=detected
vault.on("changed", handler);
