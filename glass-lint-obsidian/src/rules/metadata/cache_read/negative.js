// @case description shadowed, reassigned, dynamic, and unlisted cache accesses
// @tool glass-lint rules=obsidian:metadata.cache-read
// @expect-no-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
otherCache.getFileCache(file);

function localApp(app) {
// @expect-no-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
    app.metadataCache.getFileCache(file);
}

let cache = otherCache;
cache = otherCache;
// @expect-no-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
cache.getCache(file);

function dynamicProperty(root) {
    // @expect-no-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
    root.metadataCache[property];
}

// @expect-no-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
otherCache.getCachedFile(file);
