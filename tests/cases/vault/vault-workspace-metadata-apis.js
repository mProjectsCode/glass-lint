// @case description Rooted vault, workspace, metadata, and plugin API groups are detected
// @tool glass-lint rules=obsidian:vault.enumerate,obsidian:vault.resource-url,obsidian:workspace.active-file,obsidian:workspace.layout,obsidian:metadata.cache-read,obsidian:metadata.events,obsidian:plugins.other-access
// @tool eslint-obsidianmd config=recommended

const vault = this.app.vault;
vault.createFolder("folder");
vault.getRoot(); // @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
vault.getResourcePath(file); // @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected

const workspace = this.app.workspace;
workspace.getActiveFile(); // @expect-error glass-lint rule=obsidian:workspace.active-file message_id=detected
workspace.requestSaveLayout(); // @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected

const cache = this.app.metadataCache; // @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
cache.getFileCache(file); // @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
cache.on("changed", () => {}); // @expect-error glass-lint rule=obsidian:metadata.events message_id=detected

const plugins = this.app.plugins; // @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected line=any column=any
plugins.getPlugin("dataview");
