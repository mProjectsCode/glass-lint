// @case description direct and statically-computed postprocessor registration
// @tool glass-lint rules=obsidian:markdown.postprocessor
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:markdown.postprocessor
this.registerMarkdownPostProcessor(fn);
// @expect-error glass-lint rule=obsidian:markdown.postprocessor
this['registerMarkdownPostProcessor'](secondProcessor);

// Receiver provenance is intentionally not established by this heuristic.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:markdown.postprocessor
    this.registerMarkdownPostProcessor(unrelatedProcessor);
}
  }
}
