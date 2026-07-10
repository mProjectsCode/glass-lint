// @case description positive fixture for obsidian:metadata.cache-read
// @tool glass-lint rules=obsidian:metadata.cache-read
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app.metadataCache.getFileCache(file);
// second independent example
// @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app.metadataCache.getCache(file);
