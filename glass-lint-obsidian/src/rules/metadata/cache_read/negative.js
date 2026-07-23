// @case description shadowed, reassigned, dynamic, and unlisted cache accesses
// @tool glass-lint rules=obsidian:metadata.cache-read
// @expect-no-error glass-lint rule=obsidian:metadata.cache-read
otherCache.getFileCache(file);

function localApp(app) {
// @expect-no-error glass-lint rule=obsidian:metadata.cache-read
    app.metadataCache.getFileCache(file);
}

let cache = otherCache;
cache = otherCache;
// @expect-no-error glass-lint rule=obsidian:metadata.cache-read
cache.getCache(file);

function dynamicProperty(root) {
    // @expect-no-error glass-lint rule=obsidian:metadata.cache-read
    root.metadataCache[property];
}

// @expect-no-error glass-lint rule=obsidian:metadata.cache-read
otherCache.getCachedFile(file);
