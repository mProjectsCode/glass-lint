// @case description positive fixture for obsidian:vault.adapter
// @tool glass-lint rules=obsidian:vault.adapter
// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
app.vault.adapter;
// second independent example
// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
app.vault.adapter.exists(path);
