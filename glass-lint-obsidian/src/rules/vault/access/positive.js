// @case description positive fixture for obsidian:vault.access
// @tool glass-lint rules=obsidian:vault.access

// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
app.vault;

// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
const vaultAlias = app.vault;
// @expect-no-error glass-lint rule=obsidian:vault.access message_id=detected
vaultAlias;

// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
app.vault;
