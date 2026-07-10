// @case description positive fixture for obsidian:storage.plugin-data-read
// @tool glass-lint rules=obsidian:storage.plugin-data-read
// @expect-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this.loadData();
// second independent example
// @expect-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this.loadData();
