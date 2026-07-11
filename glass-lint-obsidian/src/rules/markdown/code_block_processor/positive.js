// @case description direct and statically-computed processor registration
// @tool glass-lint rules=obsidian:markdown.code-block-processor
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
this.registerMarkdownCodeBlockProcessor('x',fn);
// @expect-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
this['registerMarkdownCodeBlockProcessor']("second", secondProcessor);

// The heuristic does not establish that this is an Obsidian plugin instance.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
    this.registerMarkdownCodeBlockProcessor('unrelated', processor);
}
  }
}
