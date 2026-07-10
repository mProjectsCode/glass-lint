// @case description negative fixture for obsidian:file-manager.frontmatter-write
// @tool glass-lint rules=obsidian:file-manager.frontmatter-write
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
otherFileManager.processFrontMatter(file, handler);
