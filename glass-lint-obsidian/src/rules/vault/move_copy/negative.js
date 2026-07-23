// @case description shadowing, reassignment, dynamic, and unlisted methods
// @tool glass-lint rules=obsidian:vault.move-copy

// @expect-no-error glass-lint rule=obsidian:vault.move-copy
const localApp = { vault: { rename() {} } };
localApp.vault.rename(file, name);
// @expect-no-error glass-lint rule=obsidian:vault.move-copy
function shadowed(app) {
  app.vault.rename(file, name);
}
shadowed({ vault: { rename() {} } });
// @expect-no-error glass-lint rule=obsidian:vault.move-copy
app.vault[method](file, name);
// @expect-no-error glass-lint rule=obsidian:vault.move-copy
app.vault.move(file, name);

let vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.move-copy
vault.rename(file, name);
vault = localVault;
// @expect-no-error glass-lint rule=obsidian:vault.move-copy
vault.rename(file, name);
