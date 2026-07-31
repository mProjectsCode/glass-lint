// @case description rooted adapter operations, aliases, this.app, and static properties
// @tool glass-lint rules=obsidian:vault.adapter

// @expect-no-error glass-lint rule=obsidian:vault.adapter
app.vault.adapter;
// @expect-error glass-lint rule=obsidian:vault.adapter column=any
app.vault.adapter.exists(path);
// @expect-no-error glass-lint rule=obsidian:vault.adapter
const a = this.app.vault.adapter;
// @expect-no-error glass-lint rule=obsidian:vault.adapter
app["vault"]["adapter"];

const appAlias = app;
// @expect-no-error glass-lint rule=obsidian:vault.adapter
appAlias.vault.adapter;

let root = app;
// @expect-no-error glass-lint rule=obsidian:vault.adapter
root.vault.adapter;

// The later bare alias is intentionally not followed by the rooted matcher.
await a.someFutureMethod("daily.md");
