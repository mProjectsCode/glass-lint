// @case description direct, computed, same-shaped, and reassigned calls
// @tool glass-lint rules=obsidian:ui.status-bar
// @expect-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this.addStatusBarItem();

// @expect-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this["addStatusBarItem"]();

function unrelatedReceiver() {
  // @expect-error glass-lint rule=obsidian:ui.status-bar message_id=detected
  this.addStatusBarItem();
}

this.addStatusBarItem = replacement;
// @expect-error glass-lint rule=obsidian:ui.status-bar message_id=detected
this.addStatusBarItem();
