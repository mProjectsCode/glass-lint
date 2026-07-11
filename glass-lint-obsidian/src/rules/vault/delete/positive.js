// @case description configured delete APIs, aliases, this.app, and computed names
// @tool glass-lint rules=obsidian:vault.delete

// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
app.vault.delete(file);
// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
app.vault.trash(file);
// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
app.fileManager.trashFile(file);
// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
this.app.fileManager.trashFile(file);

const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
vault.delete(file);
// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
app["vault"]["trash"](file);
