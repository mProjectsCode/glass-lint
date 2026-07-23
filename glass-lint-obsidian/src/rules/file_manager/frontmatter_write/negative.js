// @case description shadowed, reassigned, dynamic, and lookalike calls
// @tool glass-lint rules=obsidian:file-manager.frontmatter-write
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write
otherFileManager.processFrontMatter(file, handler);

// A parameter named app is not the rooted provider object.
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write
function localApp(app) {
    app.fileManager.processFrontMatter(file, handler);
}

// The alias is valid before reassignment and invalid afterward.
let fileManager = app.fileManager;
fileManager = otherFileManager;
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write
fileManager.processFrontMatter(file, handler);

function dynamicProperty(property) {
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write
    app.fileManager[property](file, handler);
}

// Unlisted members are not evidence for this rule.
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write
app.fileManager.processFrontMatterAsync(file, handler);
