// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:markdown.postprocessor
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
plugin.registerMarkdownPostProcessor(handler);

const register = this.registerMarkdownPostProcessor;
// @expect-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
register(handler);

// @expect-no-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
this[dynamicMethod](handler);

// @expect-no-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
this.registerMarkdownPostProcessors(handler);
  }
}
