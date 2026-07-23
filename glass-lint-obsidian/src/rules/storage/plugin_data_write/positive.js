// @case description direct, computed, and same-shaped receiver calls
// @tool glass-lint rules=obsidian:storage.plugin-data-write
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:storage.plugin-data-write
this.saveData(data);
// @expect-error glass-lint rule=obsidian:storage.plugin-data-write
this['saveData'](secondData);

// Receiver provenance is intentionally not established by this heuristic.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write
    this.saveData(data);
}

// Reassignment invalidates the member identity.
this.saveData = replacement;
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write
this.saveData(data);
  }
}
