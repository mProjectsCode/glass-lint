// @case description direct, static-computed, and same-shaped registerView calls
// @tool glass-lint rules=obsidian:view.register
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {

// @expect-error glass-lint rule=obsidian:view.register message_id=detected
this.registerView("view", view);
// Static computed names resolve to the same syntactic chain.
// @expect-error glass-lint rule=obsidian:view.register message_id=detected
this["registerView"]("computed", view);

// The heuristic intentionally reports the same chain without proving the
// receiver is an Obsidian plugin instance.
function unrelatedReceiver() {
  // @expect-no-error glass-lint rule=obsidian:view.register message_id=detected
  this.registerView("same-shaped", view);
}

// Syntactic matching also does not track reassignment of the same member.
this.registerView = replacement;
// @expect-error glass-lint rule=obsidian:view.register message_id=detected
this.registerView("reassigned", view);
  }
}
