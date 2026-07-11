// @case description rooted receiver aliases, this.app, and static properties
// @tool glass-lint rules=obsidian:vault.access

// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
app.vault;
// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
this.app.vault;
// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
app["vault"];

const appAlias = app;
// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
appAlias.vault;

let root = app;
// @expect-error glass-lint rule=obsidian:vault.access message_id=detected
root.vault;
root = localApp;

// @expect-no-error glass-lint rule=obsidian:vault.access message_id=detected
root.vault;
