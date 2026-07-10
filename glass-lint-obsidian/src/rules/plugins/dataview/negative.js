// @case description negative fixture for obsidian:plugins.dataview
// @tool glass-lint rules=obsidian:plugins.dataview
// @expect-no-error glass-lint rule=obsidian:plugins.dataview message_id=detected
function localLookalike() { return null; }
localLookalike();
