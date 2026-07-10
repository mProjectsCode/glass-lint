// @case description positive fixture for obsidian:vault.delete
// @tool glass-lint rules=obsidian:vault.delete

// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
app.vault.delete(file);

// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
app.vault.trash(file);

const v = app.vault;
// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
v.delete(file);

// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
this.app.fileManager.trashFile(file);
