// @case description direct, computed, and reassigned syntactic chains
// @tool glass-lint rules=obsidian:ui.menu
import { Menu } from "obsidian";

// Proven module instances use the strict matcher path.
class TestMenu extends Menu {
    add() {
// @expect-error glass-lint rule=obsidian:ui.menu message_id=detected
        this.addItem(item);
    }
}

// @expect-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addItem(item);
// @expect-error glass-lint rule=obsidian:ui.menu message_id=detected
menu['addItem'](secondItem);

// Receiver provenance and reassignment are intentionally not analyzed.
menu.addItem = replacement;
// @expect-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addItem(itemAfterReassignment);
