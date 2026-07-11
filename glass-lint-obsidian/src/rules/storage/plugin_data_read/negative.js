// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:storage.plugin-data-read
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
plugin.loadData();

// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
const load = this.loadData;
load();
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this[dynamicProperty]();
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this.loadDatas();
