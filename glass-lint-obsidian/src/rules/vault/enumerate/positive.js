// @case description all configured enumeration methods and rooted aliases
// @tool glass-lint rules=obsidian:vault.enumerate

// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getFiles();
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getMarkdownFiles();
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getAllLoadedFiles();
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getAllFolders();
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getFolderByPath(path);
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
app.vault.getRoot();

const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
vault.getFiles();
// @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
this["app"]["vault"]["getRoot"]();
