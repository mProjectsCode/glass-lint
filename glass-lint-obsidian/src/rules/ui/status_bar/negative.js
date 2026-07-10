// @case description negative fixture for obsidian:ui.status-bar
// @tool glass-lint rules=obsidian:ui.status-bar
// @expect-no-error glass-lint rule=obsidian:ui.status-bar message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this.addStatusBarItems();
