// @case description negative receiver, alias, dynamic, and lookalike forms
// @tool glass-lint rules=obsidian:ui.status-bar
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:ui.status-bar
plugin.addStatusBarItem();

const addStatusBarItem = this.addStatusBarItem;
// @expect-error glass-lint rule=obsidian:ui.status-bar
addStatusBarItem();

// @expect-no-error glass-lint rule=obsidian:ui.status-bar
this[dynamicProperty]();

// @expect-no-error glass-lint rule=obsidian:ui.status-bar
this.addStatusBarItems();
  }
}
