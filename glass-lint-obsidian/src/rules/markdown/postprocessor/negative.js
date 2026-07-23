// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:markdown.postprocessor
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:markdown.postprocessor
plugin.registerMarkdownPostProcessor(handler);

const register = this.registerMarkdownPostProcessor;
// @expect-error glass-lint rule=obsidian:markdown.postprocessor
register(handler);

// @expect-no-error glass-lint rule=obsidian:markdown.postprocessor
this[dynamicMethod](handler);

// @expect-no-error glass-lint rule=obsidian:markdown.postprocessor
this.registerMarkdownPostProcessors(handler);
  }
}
