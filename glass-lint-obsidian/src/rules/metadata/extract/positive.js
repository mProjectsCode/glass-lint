// @case description positive fixture for obsidian:metadata.extract
// @tool glass-lint rules=obsidian:metadata.extract
// @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
app.metadataCache.getFileCache.tags;
// second independent example
// @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
app.metadataCache.getFileCache.links;
