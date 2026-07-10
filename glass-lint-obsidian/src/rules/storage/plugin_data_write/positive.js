// @case description positive fixture for obsidian:storage.plugin-data-write
// @tool glass-lint rules=obsidian:storage.plugin-data-write
// @expect-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this.saveData(data);
// second independent example

// @expect-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this.saveData(secondData);
