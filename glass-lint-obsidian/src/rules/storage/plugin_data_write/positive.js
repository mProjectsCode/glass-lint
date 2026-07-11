// @case description direct, computed, and same-shaped receiver calls
// @tool glass-lint rules=obsidian:storage.plugin-data-write
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this.saveData(data);
// @expect-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this['saveData'](secondData);

// Receiver provenance is intentionally not established by this heuristic.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
    this.saveData(data);
}

// Reassignment is not analyzed; the later syntactic call still matches.
this.saveData = replacement;
// @expect-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this.saveData(data);
  }
}
