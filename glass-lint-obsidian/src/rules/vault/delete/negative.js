// @case description shadowing, reassignment, dynamic, and unlisted methods
// @tool glass-lint rules=obsidian:vault.delete

// @expect-no-error glass-lint rule=obsidian:vault.delete
const localApp = { vault: { delete() {} } };
localApp.vault.delete(file);
// @expect-no-error glass-lint rule=obsidian:vault.delete
function shadowed(app) {
  app.vault.delete(file);
}
shadowed({ vault: { delete() {} } });
// @expect-no-error glass-lint rule=obsidian:vault.delete
app.vault[method](file);
// @expect-no-error glass-lint rule=obsidian:vault.delete
app.vault.rename(file, name);

let vault = app.vault;
// @expect-no-error-after glass-lint rule=obsidian:vault.delete
vault = localVault;
vault.delete(file);
