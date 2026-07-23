// @case description rooted receiver aliases, this.app, and static properties
// @tool glass-lint rules=obsidian:vault.access

// @expect-error glass-lint rule=obsidian:vault.access
app.vault;
// @expect-error glass-lint rule=obsidian:vault.access
this.app.vault;
// @expect-error glass-lint rule=obsidian:vault.access
app["vault"];

const appAlias = app;
// @expect-error glass-lint rule=obsidian:vault.access
appAlias.vault;

let root = app;
// @expect-error glass-lint rule=obsidian:vault.access
root.vault;
root = localApp;

// @expect-no-error glass-lint rule=obsidian:vault.access
root.vault;
