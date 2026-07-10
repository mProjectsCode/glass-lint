// @case description positive fixture for obsidian:vault.delete
// @tool glass-lint rules=obsidian:vault.delete
// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
app.vault.delete(file);
// second independent example
// @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
app.vault.trash(file);
