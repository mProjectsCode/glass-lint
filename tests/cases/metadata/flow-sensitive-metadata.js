// @case description Metadata frontmatter, traversal, and extraction follow connected flow
// @tool glass-lint rules=obsidian:metadata.frontmatter-read,obsidian:metadata.traversal,obsidian:metadata.extract
// @tool eslint-obsidianmd config=recommended

const cache = this.app.metadataCache.getFileCache(file);
console.log(cache.frontmatter); // @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
cache.tags; // @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
cache.links; // @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected

const links = this.app.metadataCache.resolvedLinks;
Object.entries(links); // @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
