// @case description rooted adapter reads, aliases, this.app, and static properties
// @tool glass-lint rules=obsidian:vault.adapter

// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
app.vault.adapter;
// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
app.vault.adapter.exists(path);
// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
const a = this.app.vault.adapter;
// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
app["vault"]["adapter"];

const appAlias = app;
// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
appAlias.vault.adapter;

let root = app;
// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
root.vault.adapter;

// The later bare alias is intentionally not followed by the rooted matcher.
await a.someFutureMethod("daily.md");
