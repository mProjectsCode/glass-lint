// @case description positive fixture for obsidian:vault.access
// @tool glass-lint rules=obsidian:vault.access
// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
app.vault;
// second independent example
// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
app.vault;
