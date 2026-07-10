// @case description positive fixture for obsidian:ui.ribbon
// @tool glass-lint rules=obsidian:ui.ribbon
// @expect-error glass-lint rule=obsidian:ui.ribbon message_id=detected
this.addRibbonIcon('x','x',fn);
// second independent example
// @expect-error glass-lint rule=obsidian:ui.ribbon message_id=detected
this.addRibbonIcon("second", "second", handler);
