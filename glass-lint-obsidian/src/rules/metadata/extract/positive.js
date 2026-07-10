// @case description positive fixture for obsidian:metadata.extract
// @tool glass-lint rules=obsidian:metadata.extract
// @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
app.metadataCache.getFileCache.tags;
// second independent example

// @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
app.metadataCache.getFileCache.links;

// @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
const metadataLinks = app.metadataCache.getFileCache.links;

// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
metadataLinks;
// Migrated: metadata/flow-sensitive-metadata.js and system/static-risk-apis.js
const extractCache = this.app.metadataCache.getFileCache(file);

// @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
extractCache.tags;

// @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
extractCache.links;

// @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
extractCache.embeds;
