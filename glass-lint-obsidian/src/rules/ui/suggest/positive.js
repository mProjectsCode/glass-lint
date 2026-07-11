// @case description direct, computed, same-shaped, and reassigned calls
// @tool glass-lint rules=obsidian:ui.suggest
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:ui.suggest message_id=detected
this.registerEditorSuggest(s);

// @expect-error glass-lint rule=obsidian:ui.suggest message_id=detected
this["registerEditorSuggest"](secondSuggest);

function unrelatedReceiver() {
  // @expect-no-error glass-lint rule=obsidian:ui.suggest message_id=detected
  this.registerEditorSuggest(s);
}

this.registerEditorSuggest = replacement;
// @expect-error glass-lint rule=obsidian:ui.suggest message_id=detected
this.registerEditorSuggest(s);
  }
}
