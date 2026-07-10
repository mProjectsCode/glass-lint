// @case description negative fixture for obsidian:ui.menu
// @tool glass-lint rules=obsidian:ui.menu
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addItem(item);
