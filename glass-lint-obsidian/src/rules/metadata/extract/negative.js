// @case description shadowed, reassigned, dynamic, and unlisted collections
// @tool glass-lint rules=obsidian:metadata.extract
const localMetadata = { tags: [], links: [], embeds: [] };
// @expect-no-error glass-lint rule=obsidian:metadata.extract
localMetadata.tags;

function localApp(app) {
    // @expect-no-error glass-lint rule=obsidian:metadata.extract
    app.metadataCache.getFileCache.tags;
}

let cache = app.metadataCache.getFileCache;
cache = localCache;
// @expect-no-error glass-lint rule=obsidian:metadata.extract
cache.tags;

function dynamicProperty(property) {
    // @expect-no-error glass-lint rule=obsidian:metadata.extract
    app.metadataCache.getFileCache[property];
}

// @expect-no-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.comments;
// @expect-no-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.frontmatterAuthor;
// @expect-no-error glass-lint rule=obsidian:metadata.extract
const localCache = localMetadata.getFileCache(file);
localCache.tags;
