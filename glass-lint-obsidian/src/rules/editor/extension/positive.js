// @case description direct and statically-computed registration calls
// @tool glass-lint rules=obsidian:editor.extension
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:editor.extension message_id=detected
this.registerEditorExtension(ext);
// @expect-error glass-lint rule=obsidian:editor.extension message_id=detected
this['registerEditorExtension'](secondExtension);

// The heuristic intentionally reports the same chain without proving the
// receiver is an Obsidian plugin instance.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:editor.extension message_id=detected
    this.registerEditorExtension(unrelatedExtension);
}
  }
}
