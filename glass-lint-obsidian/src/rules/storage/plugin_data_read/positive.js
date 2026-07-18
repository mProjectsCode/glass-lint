// @case description direct, computed, and same-shaped receiver calls
// @tool glass-lint rules=obsidian:storage.plugin-data-read
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this.loadData();
// @expect-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this['loadData']();

// Receiver provenance is intentionally not established by this heuristic.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
    this.loadData();
}

// Reassignment invalidates the member identity.
this.loadData = replacement;
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this.loadData();
  }
}
