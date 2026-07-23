// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:editor.extension
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// A different receiver is outside the exact syntactic chain.
// @expect-no-error glass-lint rule=obsidian:editor.extension
plugin.registerEditorExtension(ext);

// Aliases are intentionally not followed by this heuristic.
const register = this.registerEditorExtension;
// @expect-error glass-lint rule=obsidian:editor.extension
register(ext);

const method = 'registerEditorExtension';
// @expect-no-error glass-lint rule=obsidian:editor.extension
this[dynamicMethod](ext);

// @expect-no-error glass-lint rule=obsidian:editor.extension
this.registerEditorExtensions([]);
  }
}
