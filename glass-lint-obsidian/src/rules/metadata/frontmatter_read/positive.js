// @case description positive fixture for obsidian:metadata.frontmatter-read
// @tool glass-lint rules=obsidian:metadata.frontmatter-read
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
app.metadataCache.getFileCache.frontmatter;
// second independent example
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
app.metadataCache.getFileCache.frontmatter;
