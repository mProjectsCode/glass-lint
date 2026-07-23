// @case description other receivers, aliases, dynamic properties, and near-name exclusions
// @tool glass-lint rules=obsidian:view.register
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {

// A different receiver is outside the exact syntactic chain.
// @expect-no-error glass-lint rule=obsidian:view.register
plugin.registerView("other", view);

const register = this.registerView;
// Aliases are intentionally not followed by this heuristic.
// @expect-error glass-lint rule=obsidian:view.register
register("alias", view);

// @expect-no-error glass-lint rule=obsidian:view.register
this[dynamicMethod]("dynamic", view);
// @expect-no-error glass-lint rule=obsidian:view.register
this.registerViews("near-name", view);
  }
}
