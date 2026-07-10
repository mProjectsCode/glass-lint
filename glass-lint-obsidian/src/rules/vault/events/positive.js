// @case description positive fixture for obsidian:vault.events
// @tool glass-lint rules=obsidian:vault.events
// @expect-error glass-lint rule=obsidian:vault.events message_id=detected
app.vault.on('changed', fn);
// second independent example
// @expect-error glass-lint rule=obsidian:vault.events message_id=detected
app.vault.on("changed", handler);
