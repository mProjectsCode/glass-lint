// @case description negative fixture for obsidian:ui.modal
// @tool glass-lint rules=obsidian:ui.modal
// @expect-no-error glass-lint rule=obsidian:ui.modal message_id=detected
function localLookalike() { return null; }
localLookalike();
