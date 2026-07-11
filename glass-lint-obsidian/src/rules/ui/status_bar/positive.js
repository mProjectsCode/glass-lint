// @case description direct, computed, same-shaped, and reassigned calls
// @tool glass-lint rules=obsidian:ui.status-bar
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this.addStatusBarItem();

// @expect-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this["addStatusBarItem"]();

function unrelatedReceiver() {
  // @expect-no-error glass-lint rule=obsidian:ui.status-bar message_id=detected
  this.addStatusBarItem();
}

this.addStatusBarItem = replacement;
// @expect-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this.addStatusBarItem();
  }
}
