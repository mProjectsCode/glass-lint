// @case description shadowing, reassignment, dynamic, and unlisted methods
// @tool glass-lint rules=obsidian:vault.read

// @expect-no-error glass-lint rule=obsidian:vault.read
const localApp = { vault: { read() {} } };
localApp.vault.read(file);
// @expect-no-error glass-lint rule=obsidian:vault.read
function shadowed(app) {
  app.vault.read(file);
}
shadowed({ vault: { read() {} } });
// @expect-no-error glass-lint rule=obsidian:vault.read
app.vault[method](file);
// @expect-no-error glass-lint rule=obsidian:vault.read
app.vault.readJson(file);

let vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.read
vault.read(file);
vault = localVault;
// @expect-no-error glass-lint rule=obsidian:vault.read
vault.read(file);
