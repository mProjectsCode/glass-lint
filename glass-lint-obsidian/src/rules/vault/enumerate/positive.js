// @case description all configured enumeration methods and rooted aliases
// @tool glass-lint rules=obsidian:vault.enumerate

// @expect-error glass-lint rule=obsidian:vault.enumerate
app.vault.getFiles();
// @expect-error glass-lint rule=obsidian:vault.enumerate
app.vault.getMarkdownFiles();
// @expect-error glass-lint rule=obsidian:vault.enumerate
app.vault.getAllLoadedFiles();
// @expect-error glass-lint rule=obsidian:vault.enumerate
app.vault.getAllFolders();
// @expect-error glass-lint rule=obsidian:vault.enumerate
app.vault.getFolderByPath(path);
// @expect-error glass-lint rule=obsidian:vault.enumerate
app.vault.getRoot();
// @expect-error glass-lint rule=obsidian:vault.enumerate
app.vault.getFileByPath(path);
// @expect-error glass-lint rule=obsidian:vault.enumerate
app.vault.getAbstractFileByPath(path);
// @expect-error glass-lint rule=obsidian:vault.enumerate
app.vault.recurseChildren(folder, callback);

const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.enumerate
vault.getFiles();
// @expect-error glass-lint rule=obsidian:vault.enumerate
this["app"]["vault"]["getRoot"]();
