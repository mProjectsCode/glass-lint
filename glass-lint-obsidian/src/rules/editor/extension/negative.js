// @case description negative fixture for obsidian:editor.extension
// @tool glass-lint rules=obsidian:editor.extension
// @expect-no-error glass-lint rule=obsidian:editor.extension message_id=detected
function localLookalike() { return null; }
localLookalike();
