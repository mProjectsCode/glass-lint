// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:markdown.code-block-processor
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
plugin.registerMarkdownCodeBlockProcessor('x', handler);

const register = this.registerMarkdownCodeBlockProcessor;
// @expect-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
register('x', handler);

// @expect-no-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
this[dynamicMethod]('x', handler);

// @expect-no-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
this.registerMarkdownProcessor(handler);
  }
}
