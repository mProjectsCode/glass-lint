// @case description negative receiver, alias, dynamic, and lookalike forms
// @tool glass-lint rules=obsidian:ui.status-bar
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:ui.status-bar message_id=detected
plugin.addStatusBarItem();

const addStatusBarItem = this.addStatusBarItem;
// @expect-no-error glass-lint rule=obsidian:ui.status-bar message_id=detected
addStatusBarItem();

// @expect-no-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this[dynamicProperty]();

// @expect-no-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this.addStatusBarItems();
  }
}
