// @case description Vault file mutations are detected
// @tool glass-lint rules=obsidian:workspace.open,obsidian:vault.write,obsidian:vault.delete,obsidian:vault.move-copy,obsidian:metadata.frontmatter-write,obsidian:workspace.leaf-management
// @tool eslint-obsidianmd config=recommended

this.app.workspace.getLeaf(false).openFile(file); // @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
this.app.vault.createFolder("new"); // @expect-error glass-lint rule=obsidian:vault.write message_id=detected
this.app.vault.appendBinary(file, data); // @expect-error glass-lint rule=obsidian:vault.write message_id=detected
this.app.vault.process(file, data => data); // @expect-error glass-lint rule=obsidian:vault.write message_id=detected
this.app.fileManager.renameFile(file, "renamed.md"); // @expect-error glass-lint rule=obsidian:vault.move-copy message_id=detected
this.app.fileManager.trashFile(file); // @expect-error glass-lint rule=obsidian:vault.delete message_id=detected
this.app.fileManager.processFrontMatter(file, data => { data.done = true; }); // @expect-error glass-lint rule=obsidian:metadata.frontmatter-write message_id=detected
