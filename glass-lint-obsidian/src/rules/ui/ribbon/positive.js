// @case description direct, computed, same-shaped, and reassigned calls
// @tool glass-lint rules=obsidian:ui.ribbon
// @expect-error glass-lint rule=obsidian:ui.ribbon message_id=detected
this.addRibbonIcon("x", "x", fn);

// @expect-error glass-lint rule=obsidian:ui.ribbon message_id=detected
this["addRibbonIcon"]("computed", "computed", handler);

function unrelatedReceiver() {
  // @expect-error glass-lint rule=obsidian:ui.ribbon message_id=detected
  this.addRibbonIcon("same-shaped", "same-shaped", handler);
}

this.addRibbonIcon = replacement;
// @expect-error glass-lint rule=obsidian:ui.ribbon message_id=detected
this.addRibbonIcon("reassigned", "reassigned", handler);
