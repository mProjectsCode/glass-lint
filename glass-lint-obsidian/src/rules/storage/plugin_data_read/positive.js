// @case description direct, computed, and same-shaped receiver calls
// @tool glass-lint rules=obsidian:storage.plugin-data-read
// @expect-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this.loadData();
// @expect-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this['loadData']();

// Receiver provenance is intentionally not established by this heuristic.
function unrelatedReceiver() {
    // @expect-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
    this.loadData();
}

// Reassignment is not analyzed; the later syntactic call still matches.
this.loadData = replacement;
// @expect-error glass-lint rule=obsidian:storage.plugin-data-read message_id=detected
this.loadData();
