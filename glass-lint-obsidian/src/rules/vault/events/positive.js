// @case description rooted vault event registration and static aliases
// @tool glass-lint rules=obsidian:vault.events

// @expect-error glass-lint rule=obsidian:vault.events
app.vault.on('create', fn);
const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.events
vault.on("modify", fn);
// @expect-error glass-lint rule=obsidian:vault.events
app.vault.on("delete", fn);
// @expect-error glass-lint rule=obsidian:vault.events
app.vault.on("rename", fn);
