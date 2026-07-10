// @case description negative fixture for obsidian:ui.ribbon
// @tool glass-lint rules=obsidian:ui.ribbon
// @expect-no-error glass-lint rule=obsidian:ui.ribbon message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:ui.ribbon message_id=detected
this.addRibbonIconButton("x");
