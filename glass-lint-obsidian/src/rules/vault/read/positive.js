// @case description positive fixture for obsidian:vault.read
// @tool glass-lint rules=obsidian:vault.read
// @expect-error glass-lint rule=obsidian:vault.read message_id=detected
app.vault.read(file);
// second independent example
// @expect-error glass-lint rule=obsidian:vault.read message_id=detected
app.vault.cachedRead(file);
