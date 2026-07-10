// @case description positive fixture for obsidian:metadata.frontmatter-write
// @tool glass-lint rules=obsidian:metadata.frontmatter-write
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-write message_id=detected
app.fileManager.processFrontMatter(file, fn);
// second independent example
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-write message_id=detected
app.fileManager.processFrontMatter(otherFile, handler);
