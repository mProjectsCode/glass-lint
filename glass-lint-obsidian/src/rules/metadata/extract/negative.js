// @case description shadowed, reassigned, dynamic, and unlisted collections
// @tool glass-lint rules=obsidian:metadata.extract
const localMetadata = { tags: [], links: [], embeds: [] };
// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
localMetadata.tags;

function localApp(app) {
    // @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
    app.metadataCache.getFileCache.tags;
}

let cache = app.metadataCache.getFileCache;
cache = localCache;
// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
cache.tags;

function dynamicProperty(property) {
    // @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
    app.metadataCache.getFileCache[property];
}

// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
app.metadataCache.getFileCache.comments;
// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
app.metadataCache.getFileCache.frontmatterAuthor;
// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
const localCache = localMetadata.getFileCache(file);
localCache.tags;
