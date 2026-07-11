// @case description all configured cache reads and calls
// @tool glass-lint rules=obsidian:metadata.cache-read
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app.metadataCache;
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app.metadataCache.resolvedLinks;
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app.metadataCache.unresolvedLinks;

// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app.metadataCache.getFileCache(file);
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app.metadataCache.getCache(file);
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app.metadataCache.getFirstLinkpathDest(link, source);

// Rooted aliases retain their cache provenance.
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
const metadataCache = this.app.metadataCache;
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
metadataCache.getFileCache(file);

// Static computed properties retain the same rooted chain.
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app['metadataCache']['getCache'](file);
