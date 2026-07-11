// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:editor.suggest
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:editor.suggest message_id=detected
plugin.registerEditorSuggest(s);

const register = this.registerEditorSuggest;
// @expect-no-error glass-lint rule=obsidian:editor.suggest message_id=detected
register(s);

// @expect-no-error glass-lint rule=obsidian:editor.suggest message_id=detected
this[dynamicMethod](s);

// @expect-no-error glass-lint rule=obsidian:editor.suggest message_id=detected
this.registerEditorSuggestion(handler);
  }
}
