// @case description direct, computed, and reassigned syntactic chains
// @tool glass-lint rules=obsidian:ui.menu
import { Menu } from "obsidian";

// Proven module instances use the strict matcher path.
class TestMenu extends Menu {
    add() {
// @expect-error glass-lint rule=obsidian:ui.menu
        this.addItem(item);
    }
}

// Chained new-expression instances are a known regression: member_call_instance
// does not track the result of `new Menu()` through the method call chain.
// @expect-error glass-lint rule=obsidian:ui.menu
new Menu().addItem(item);

// Unproven bare receivers are intentionally excluded.
// @expect-no-error glass-lint rule=obsidian:ui.menu
menu.addItem(item);
// @expect-no-error glass-lint rule=obsidian:ui.menu
menu['addItem'](secondItem);

// Unproven receiver provenance and reassignment are excluded.
menu.addItem = replacement;
// @expect-no-error glass-lint rule=obsidian:ui.menu
menu.addItem(itemAfterReassignment);
