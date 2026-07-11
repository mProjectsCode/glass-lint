// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:ui.menu
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
otherMenu.addMenuItem(item);

// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
const addItem = menu.addMenuItem;
addItem(item);
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
menu[dynamicProperty](item);
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addMenuItems(item);
