// @case description shadowing, reassignment, dynamic, and unlisted methods
// @tool glass-lint rules=obsidian:vault.delete

// @expect-no-error glass-lint rule=obsidian:vault.delete message_id=detected
const localApp = { vault: { delete() {} } };
localApp.vault.delete(file);
// @expect-no-error glass-lint rule=obsidian:vault.delete message_id=detected
function shadowed(app) {
  app.vault.delete(file);
}
shadowed({ vault: { delete() {} } });
// @expect-no-error glass-lint rule=obsidian:vault.delete message_id=detected
app.vault[method](file);
// @expect-no-error glass-lint rule=obsidian:vault.delete message_id=detected
app.vault.rename(file, name);

let vault = app.vault;
// @expect-no-error-after glass-lint rule=obsidian:vault.delete message_id=detected
vault = localVault;
vault.delete(file);
