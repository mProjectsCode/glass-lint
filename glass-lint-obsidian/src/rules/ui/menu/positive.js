// @case description positive fixture for obsidian:ui.menu
// @tool glass-lint rules=obsidian:ui.menu
// @expect-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addMenuItem(item);
// second independent example

// @expect-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addMenuItem(item);
