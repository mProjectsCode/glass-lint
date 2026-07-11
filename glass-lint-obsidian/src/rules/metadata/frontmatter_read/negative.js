// @case description shadowed, reassigned, dynamic, and unlisted frontmatter reads
// @tool glass-lint rules=obsidian:metadata.frontmatter-read
// @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
otherCache.getFileCache.frontmatter;

function localApp(app) {
    // @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
    app.metadataCache.getFileCache.frontmatter;
}

let cache = app.metadataCache.getFileCache;
cache = localCache;
// @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
cache.frontmatter;

function dynamicProperty(property) {
    // @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
    app.metadataCache.getFileCache[property];
}

// @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
app.metadataCache.getFileCache.description;
// @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
const localCache = localMetadata.getFileCache(file);
localCache.frontmatter;
