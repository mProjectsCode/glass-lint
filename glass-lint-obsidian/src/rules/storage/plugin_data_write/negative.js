// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:storage.plugin-data-write
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
plugin.saveData(data);

// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
const save = this.saveData;
save(data);
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this[dynamicProperty](data);
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this.saveDatas(data);
