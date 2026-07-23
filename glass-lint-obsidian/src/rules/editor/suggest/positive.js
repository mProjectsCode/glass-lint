// @case description direct and statically-computed suggestion registration
// @tool glass-lint rules=obsidian:editor.suggest
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:editor.suggest
this.registerEditorSuggest(s);
// @expect-error glass-lint rule=obsidian:editor.suggest
this['registerEditorSuggest'](secondSuggest);

function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:editor.suggest
    this.registerEditorSuggest(unrelatedSuggest);
}
  }
}
