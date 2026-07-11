// @case description direct, computed, and same-shaped receiver calls
// @tool glass-lint rules=obsidian:ui.command
// @expect-error glass-lint rule=obsidian:ui.command message_id=detected
this.addCommand({id:'x'});
// @expect-error glass-lint rule=obsidian:ui.command message_id=detected
this['addCommand']({ id: "second" });

// Receiver provenance is intentionally not established by this heuristic.
function unrelatedReceiver() {
    // @expect-error glass-lint rule=obsidian:ui.command message_id=detected
    this.addCommand({ id: "unrelated" });
}

// Reassignment is not analyzed; the later syntactic call still matches.
this.addCommand = replacement;
// @expect-error glass-lint rule=obsidian:ui.command message_id=detected
this.addCommand({ id: "reassigned" });
