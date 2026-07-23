// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:ui.menu
// @expect-no-error glass-lint rule=obsidian:ui.menu
otherMenu.addItem(item);

function localMenu(menu) {
    // @expect-no-error glass-lint rule=obsidian:ui.menu
    menu.addItem(item);
}

// @expect-no-error glass-lint rule=obsidian:ui.menu
const addItem = menu.addItem;
addItem(item);
// @expect-no-error glass-lint rule=obsidian:ui.menu
menu[dynamicProperty](item);
// @expect-no-error glass-lint rule=obsidian:ui.menu
menu.addItems(item);

// The old non-public spelling is not part of the Obsidian Menu API.
// @expect-no-error glass-lint rule=obsidian:ui.menu
menu.addMenuItem(item);
