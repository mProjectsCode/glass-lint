// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:ui.command
// @expect-no-error glass-lint rule=obsidian:ui.command message_id=detected
plugin.addCommand(command);

// @expect-no-error glass-lint rule=obsidian:ui.command message_id=detected
const add = this.addCommand;
add(command);
// @expect-no-error glass-lint rule=obsidian:ui.command message_id=detected
this[dynamicProperty](command);
// @expect-no-error glass-lint rule=obsidian:ui.command message_id=detected
this.addCommands(command);
