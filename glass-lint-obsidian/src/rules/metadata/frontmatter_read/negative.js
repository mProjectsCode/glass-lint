// @case description negative fixture for obsidian:metadata.frontmatter-read
// @tool glass-lint rules=obsidian:metadata.frontmatter-read
// @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
app.metadataCache.getFileCache.description;
