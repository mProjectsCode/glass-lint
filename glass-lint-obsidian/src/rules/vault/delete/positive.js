// @case description configured delete APIs, aliases, this.app, and computed names
// @tool glass-lint rules=obsidian:vault.delete

// @expect-error glass-lint rule=obsidian:vault.delete
app.vault.delete(file);
// @expect-error glass-lint rule=obsidian:vault.delete
app.vault.trash(file);
// @expect-error glass-lint rule=obsidian:vault.delete
app.fileManager.trashFile(file);
// @expect-error glass-lint rule=obsidian:vault.delete
this.app.fileManager.trashFile(file);

const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.delete
vault.delete(file);
// @expect-error glass-lint rule=obsidian:vault.delete
app["vault"]["trash"](file);
