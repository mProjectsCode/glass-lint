// @case description shadowing, reassignment, dynamic, and unlisted methods
// @tool glass-lint rules=obsidian:vault.enumerate

// @expect-no-error glass-lint rule=obsidian:vault.enumerate message_id=detected
const localApp = { vault: { getFiles() {} } };
localApp.vault.getFiles();
// @expect-no-error glass-lint rule=obsidian:vault.enumerate message_id=detected
function shadowed(app) {
  app.vault.getFiles();
}
shadowed({ vault: { getFiles() {} } });
// @expect-no-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault[method]();
// @expect-no-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getAbstractFileByPath(path);

let vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
vault.getFiles();
vault = localVault;
// @expect-no-error glass-lint rule=obsidian:vault.enumerate message_id=detected
vault.getFiles();
