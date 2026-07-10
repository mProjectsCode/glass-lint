// @case description positive fixture for obsidian:vault.write
// @tool glass-lint rules=obsidian:vault.write
// @expect-error glass-lint rule=obsidian:vault.write message_id=detected
app.vault.modify(file, text);
// second independent example
// @expect-error glass-lint rule=obsidian:vault.write message_id=detected
app.vault.append(file, text);
