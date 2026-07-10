// @case description negative fixture for obsidian:ui.command
// @tool glass-lint rules=obsidian:ui.command
// @expect-no-error glass-lint rule=obsidian:ui.command message_id=detected
function localLookalike() { return null; }
localLookalike();
