// @case description direct, computed, and reassigned syntactic chains
// @tool glass-lint rules=obsidian:ui.menu
// @expect-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addMenuItem(item);
// @expect-error glass-lint rule=obsidian:ui.menu message_id=detected
menu['addMenuItem'](secondItem);

// Receiver provenance and reassignment are intentionally not analyzed.
menu.addMenuItem = replacement;
// @expect-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addMenuItem(itemAfterReassignment);
