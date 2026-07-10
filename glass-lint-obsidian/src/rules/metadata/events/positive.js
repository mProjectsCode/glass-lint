// @case description positive fixture for obsidian:metadata.events
// @tool glass-lint rules=obsidian:metadata.events
// @expect-error glass-lint rule=obsidian:metadata.events message_id=detected
app.metadataCache.on('changed', fn);
// second independent example
// @expect-error glass-lint rule=obsidian:metadata.events message_id=detected
app.metadataCache.on("resolved", handler);
const metadataCache = app.metadataCache;
// @expect-error glass-lint rule=obsidian:metadata.events message_id=detected
metadataCache.on("changed", handler);

// Migrated: vault/vault-workspace-metadata-apis.js
const legacyEventCache = this.app.metadataCache;
// @expect-error glass-lint rule=obsidian:metadata.events message_id=detected
legacyEventCache.on("changed", () => {});
