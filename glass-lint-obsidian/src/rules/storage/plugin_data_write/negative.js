// @case description negative fixture for obsidian:storage.plugin-data-write
// @tool glass-lint rules=obsidian:storage.plugin-data-write
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this.savePluginData(data);
