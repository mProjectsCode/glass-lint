// @case description configured move-copy APIs, aliases, this.app, and computed names
// @tool glass-lint rules=obsidian:vault.move-copy

// @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
app.vault.rename(file, name);
// @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
app.vault.copy(file, destination);
// @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
app.fileManager.renameFile(file, "renamed.md");
// @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
this.app.fileManager.renameFile(file, "renamed-again.md");

const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
vault.rename(file, name);
// @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
app["vault"]["copy"](file, destination);
