// @case description positive fixture for obsidian:ui.status-bar
// @tool glass-lint rules=obsidian:ui.status-bar
// @expect-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this.addStatusBarItem();
// second independent example

// @expect-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this.addStatusBarItem();
