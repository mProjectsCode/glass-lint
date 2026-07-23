// @case description rooted adapter reads, aliases, this.app, and static properties
// @tool glass-lint rules=obsidian:vault.adapter

// @expect-error glass-lint rule=obsidian:vault.adapter
app.vault.adapter;
// @expect-error glass-lint rule=obsidian:vault.adapter
app.vault.adapter.exists(path);
// @expect-error glass-lint rule=obsidian:vault.adapter
const a = this.app.vault.adapter;
// @expect-error glass-lint rule=obsidian:vault.adapter
app["vault"]["adapter"];

const appAlias = app;
// @expect-error glass-lint rule=obsidian:vault.adapter
appAlias.vault.adapter;

let root = app;
// @expect-error glass-lint rule=obsidian:vault.adapter
root.vault.adapter;

// The later bare alias is intentionally not followed by the rooted matcher.
await a.someFutureMethod("daily.md");
