// @case description positive fixture for obsidian:metadata.frontmatter-read
// @tool glass-lint rules=obsidian:metadata.frontmatter-read
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
app.metadataCache.getFileCache.frontmatter;

// Migrated: metadata/flow-sensitive-metadata.js
const legacyMetadataCache = this.app.metadataCache.getFileCache(file);
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
legacyMetadataCache.frontmatter;
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
const frontmatter = app.metadataCache.getFileCache.frontmatter;
// @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
frontmatter;
// second independent example
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
app.metadataCache.getFileCache.frontmatter;
