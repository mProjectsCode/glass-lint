// @case description configured move-copy APIs, aliases, this.app, and computed names
// @tool glass-lint rules=obsidian:vault.move-copy

// @expect-error glass-lint rule=obsidian:vault.move-copy
app.vault.rename(file, name);
// @expect-error glass-lint rule=obsidian:vault.move-copy
app.vault.copy(file, destination);
// @expect-error glass-lint rule=obsidian:vault.move-copy
app.fileManager.renameFile(file, "renamed.md");
// @expect-error glass-lint rule=obsidian:vault.move-copy
this.app.fileManager.renameFile(file, "renamed-again.md");

const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.move-copy
vault.rename(file, name);
// @expect-error glass-lint rule=obsidian:vault.move-copy
app["vault"]["copy"](file, destination);
