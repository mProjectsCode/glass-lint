// @case description Vault file mutations are detected
// @tool glass-lint rules=obsidian:vault.open_create_flows,obsidian:vault.write,obsidian:vault.destructive,obsidian:metadata.frontmatter_write,obsidian:workspace.views

this.app.workspace.getLeaf(false).openFile(file); // @expect-error glass-lint rule=obsidian:vault.open_create_flows message_id=detected
// @expect-error-after glass-lint rule=obsidian:workspace.views message_id=detected
this.app.vault.createFolder("new"); // @expect-error glass-lint rule=obsidian:vault.write message_id=detected
this.app.vault.appendBinary(file, data); // @expect-error glass-lint rule=obsidian:vault.write message_id=detected
this.app.vault.process(file, data => data); // @expect-error glass-lint rule=obsidian:vault.write message_id=detected
this.app.fileManager.renameFile(file, "renamed.md"); // @expect-error glass-lint rule=obsidian:vault.destructive message_id=detected
this.app.fileManager.trashFile(file); // @expect-error glass-lint rule=obsidian:vault.destructive message_id=detected
this.app.fileManager.processFrontMatter(file, data => { data.done = true; }); // @expect-error glass-lint rule=obsidian:metadata.frontmatter_write message_id=detected
