// @case description positive fixture for obsidian:ui.command
// @tool glass-lint rules=obsidian:ui.command
// @expect-error glass-lint rule=obsidian:ui.command message_id=detected
this.addCommand({id:'x'});
// second independent example
// @expect-error glass-lint rule=obsidian:ui.command message_id=detected
this.addCommand({ id: "second" });
