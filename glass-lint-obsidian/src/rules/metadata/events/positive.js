// @case description configured events, rooted aliases, and static event names
// @tool glass-lint rules=obsidian:metadata.events
// @expect-error glass-lint rule=obsidian:metadata.events
app.metadataCache.on('changed', fn);
// @expect-error glass-lint rule=obsidian:metadata.events
app.metadataCache.on("deleted", handler);
// @expect-error glass-lint rule=obsidian:metadata.events
app.metadataCache.on("resolved", handler);
// @expect-error glass-lint rule=obsidian:metadata.events
app.metadataCache.on("resolve", handler);

const metadataCache = app.metadataCache;
// @expect-error glass-lint rule=obsidian:metadata.events
metadataCache.on("changed", handler);

const eventCache = this.app.metadataCache;
// @expect-error glass-lint rule=obsidian:metadata.events
eventCache.on("changed", () => {});
