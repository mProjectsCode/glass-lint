// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:lifecycle.events
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:lifecycle.events
plugin.registerEvent(handler);

const register = this.registerEvent;
// @expect-error glass-lint rule=obsidian:lifecycle.events
register(handler);

// @expect-no-error glass-lint rule=obsidian:lifecycle.events
this[dynamicMethod](handler);

// @expect-no-error glass-lint rule=obsidian:lifecycle.events
this.registerEventual(handler);

// @expect-no-error glass-lint rule=obsidian:lifecycle.events
this.registerDomEvents(element, handler);
  }
}
