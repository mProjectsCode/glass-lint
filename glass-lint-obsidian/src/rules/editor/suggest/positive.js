// @case description direct and statically-computed suggestion registration
// @tool glass-lint rules=obsidian:editor.suggest
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:editor.suggest message_id=detected
this.registerEditorSuggest(s);
// @expect-error glass-lint rule=obsidian:editor.suggest message_id=detected
this['registerEditorSuggest'](secondSuggest);

// The syntactic heuristic does not establish the receiver's provider type.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:editor.suggest message_id=detected
    this.registerEditorSuggest(unrelatedSuggest);
}
  }
}
