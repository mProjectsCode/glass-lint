// @case description Rooted vault, workspace, metadata, and plugin API groups are detected
// @tool glass-lint rules=obsidian:vault.folder_ops,obsidian:vault.resources,obsidian:workspace.active_file,obsidian:workspace.layout_persistence,obsidian:metadata.read,obsidian:metadata.events,obsidian:plugins.internal_access

const vault = this.app.vault;
vault.createFolder("folder");
vault.getRoot(); // @expect-error glass-lint rule=obsidian:vault.folder_ops message_id=detected
vault.getResourcePath(file); // @expect-error glass-lint rule=obsidian:vault.resources message_id=detected

const workspace = this.app.workspace;
workspace.getActiveFile(); // @expect-error glass-lint rule=obsidian:workspace.active_file message_id=detected
workspace.requestSaveLayout(); // @expect-error glass-lint rule=obsidian:workspace.layout_persistence message_id=detected

const cache = this.app.metadataCache; // @expect-error glass-lint rule=obsidian:metadata.read message_id=detected
cache.getFileCache(file); // @expect-error glass-lint rule=obsidian:metadata.read message_id=detected
cache.on("changed", () => {}); // @expect-error glass-lint rule=obsidian:metadata.events message_id=detected

const plugins = this.app.plugins; // @expect-error glass-lint rule=obsidian:plugins.internal_access message_id=detected
plugins.getPlugin("dataview"); // @expect-error glass-lint rule=obsidian:plugins.internal_access message_id=detected
