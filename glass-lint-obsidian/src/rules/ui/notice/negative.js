// @case description negative fixture for obsidian:ui.notice
// @tool glass-lint rules=obsidian:ui.notice
// @expect-no-error glass-lint rule=obsidian:ui.notice message_id=detected
function localLookalike() { return null; }
localLookalike();
