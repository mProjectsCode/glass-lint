// @case description rooted vault event registration and static aliases
// @tool glass-lint rules=obsidian:vault.events

// @expect-error glass-lint rule=obsidian:vault.events message_id=detected
app.vault.on('create', fn);
// @expect-error glass-lint rule=obsidian:vault.events message_id=detected
this.app.vault["on"]("closed", fn);
const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.events message_id=detected
vault.on("modify", fn);
