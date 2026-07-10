// @case description negative fixture for obsidian:storage.plugin-data-read
// @tool glass-lint rules=obsidian:storage.plugin-data-read
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
function localLookalike() { return null; }
localLookalike();
