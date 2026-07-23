// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:editor.suggest
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:editor.suggest
plugin.registerEditorSuggest(s);

const register = this.registerEditorSuggest;
// @expect-error glass-lint rule=obsidian:editor.suggest
register(s);

// @expect-no-error glass-lint rule=obsidian:editor.suggest
this[dynamicMethod](s);

// @expect-no-error glass-lint rule=obsidian:editor.suggest
this.registerEditorSuggestion(handler);
  }
}
