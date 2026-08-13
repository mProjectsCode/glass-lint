// @case description direct and statically-computed registration calls
// @tool glass-lint rules=obsidian:editor.extension
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:editor.extension
this.registerEditorExtension(ext);
// @expect-error glass-lint rule=obsidian:editor.extension
this['registerEditorExtension'](secondExtension);

// A same-shaped receiver is correctly excluded without plugin-instance
// provenance.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:editor.extension
    this.registerEditorExtension(unrelatedExtension);
}
  }
}
