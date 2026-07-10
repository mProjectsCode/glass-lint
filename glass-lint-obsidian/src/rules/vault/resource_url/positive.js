// @case description positive fixture for obsidian:vault.resource-url
// @tool glass-lint rules=obsidian:vault.resource-url
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
app.vault.getResourcePath(file);
// second independent example
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
app.vault.getResourcePath(otherFile);
