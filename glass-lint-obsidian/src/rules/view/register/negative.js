// @case description other receivers, aliases, dynamic properties, and near-name exclusions
// @tool glass-lint rules=obsidian:view.register
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {

// A different receiver is outside the exact syntactic chain.
// @expect-no-error glass-lint rule=obsidian:view.register message_id=detected
plugin.registerView("other", view);

const register = this.registerView;
// Aliases are intentionally not followed by this heuristic.
// @expect-error glass-lint rule=obsidian:view.register message_id=detected
register("alias", view);

// @expect-no-error glass-lint rule=obsidian:view.register message_id=detected
this[dynamicMethod]("dynamic", view);
// @expect-no-error glass-lint rule=obsidian:view.register message_id=detected
this.registerViews("near-name", view);
  }
}
