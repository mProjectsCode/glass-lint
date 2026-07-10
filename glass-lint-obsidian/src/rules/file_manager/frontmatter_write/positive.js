// @case description positive fixture for obsidian:file-manager.frontmatter-write
// @tool glass-lint rules=obsidian:file-manager.frontmatter-write
// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
app.fileManager.processFrontMatter(file, fn);
// second independent example

// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
app.fileManager.processFrontMatter(otherFile, handler);
const manager = app.fileManager;

// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
manager.processFrontMatter(file, fn);
// Migrated: vault/open-create-and-mutations.js

// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
this.app.fileManager.processFrontMatter(file, data => { data.legacy = true; });
