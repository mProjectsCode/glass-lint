// @case description direct, computed, and same-shaped receiver calls
// @tool glass-lint rules=obsidian:ui.command
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:ui.command
this.addCommand({id:'x'});
// @expect-error glass-lint rule=obsidian:ui.command
this['addCommand']({ id: "second" });

const add = this.addCommand;
// @expect-error glass-lint rule=obsidian:ui.command
add({ id: "alias" });
const bound = this.addCommand.bind(this);
// @expect-error glass-lint rule=obsidian:ui.command
bound({ id: "bound" });

// Receiver provenance is intentionally not established by this heuristic.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:ui.command
    this.addCommand({ id: "unrelated" });
}

// Reassignment invalidates the extracted/direct callable identity.
this.addCommand = replacement;
// @expect-no-error glass-lint rule=obsidian:ui.command
this.addCommand({ id: "reassigned" });
  }
}
