// @case description positive fixture for obsidian:vault.enumerate
// @tool glass-lint rules=obsidian:vault.enumerate
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getFiles();
// second independent example
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getMarkdownFiles();
