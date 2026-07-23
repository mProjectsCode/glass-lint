// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:markdown.code-block-processor
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:markdown.code-block-processor
plugin.registerMarkdownCodeBlockProcessor('x', handler);

const register = this.registerMarkdownCodeBlockProcessor;
// @expect-error glass-lint rule=obsidian:markdown.code-block-processor
register('x', handler);

// @expect-no-error glass-lint rule=obsidian:markdown.code-block-processor
this[dynamicMethod]('x', handler);

// @expect-no-error glass-lint rule=obsidian:markdown.code-block-processor
this.registerMarkdownProcessor(handler);
  }
}
