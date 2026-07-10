// @case description positive fixture for obsidian:vault.enumerate
// @tool glass-lint rules=obsidian:vault.enumerate

// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getFiles();

// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getMarkdownFiles();
const v1 = app.vault;

// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
v1.getFiles();
const v2 = {};
v2.vault = this.app.vault;

// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
v2.vault.getFiles();
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
this.app.vault.getRoot();
