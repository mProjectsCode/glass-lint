// @case description rooted direct calls, aliases, and static properties
// @tool glass-lint rules=obsidian:file-manager.frontmatter-write
// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write
app.fileManager.processFrontMatter(file, fn);
// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write
this.app.fileManager.processFrontMatter(otherFile, handler);

const manager = app.fileManager;
// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write
manager.processFrontMatter(file, fn);

const { processFrontMatter: updateFrontmatter } = app.fileManager;
// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write
updateFrontmatter(file, fn);

let managerBeforeReassignment = app.fileManager;
// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write
managerBeforeReassignment.processFrontMatter(file, handler);
managerBeforeReassignment = otherFileManager;

// Static computed properties resolve to the same rooted API.
// @expect-error glass-lint rule=obsidian:file-manager.frontmatter-write
app['fileManager']['processFrontMatter'](file, handler);
