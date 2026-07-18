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

// Unproven bare receivers are intentionally excluded.
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addItem(item);
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
menu['addItem'](secondItem);

// Unproven receiver provenance and reassignment are excluded.
menu.addItem = replacement;
// @expect-no-error glass-lint rule=obsidian:ui.menu message_id=detected
menu.addItem(itemAfterReassignment);
