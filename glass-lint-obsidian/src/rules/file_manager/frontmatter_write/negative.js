// @case description shadowed, reassigned, dynamic, and lookalike calls
// @tool glass-lint rules=obsidian:file-manager.frontmatter-write
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
otherFileManager.processFrontMatter(file, handler);

// A parameter named app is not the rooted provider object.
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
function localApp(app) {
    app.fileManager.processFrontMatter(file, handler);
}

// The alias is valid before reassignment and invalid afterward.
let fileManager = app.fileManager;
fileManager = otherFileManager;
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
fileManager.processFrontMatter(file, handler);

function dynamicProperty(property) {
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
    app.fileManager[property](file, handler);
}

// Unlisted members are not evidence for this rule.
// @expect-no-error glass-lint rule=obsidian:file-manager.frontmatter-write message_id=detected
app.fileManager.processFrontMatterAsync(file, handler);
