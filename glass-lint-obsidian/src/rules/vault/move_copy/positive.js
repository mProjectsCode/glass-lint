// @case description positive fixture for obsidian:vault.move-copy
// @tool glass-lint rules=obsidian:vault.move-copy

// @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
app.vault.rename(file, name);

// @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
app.vault.copy(file, destination);

// @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
this.app.fileManager.renameFile(file, "renamed.md");
