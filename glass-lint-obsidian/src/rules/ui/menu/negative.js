// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:ui.menu
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
otherMenu.addItem(item);

// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
const addItem = menu.addItem;
addItem(item);
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
menu[dynamicProperty](item);
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addItems(item);

// The old non-public spelling is not part of the Obsidian Menu API.
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addMenuItem(item);
