// @case description Plain commands and event-name strings do not report editor integration
// @tool glass-lint rules=obsidian:ui.command
// @tool eslint-obsidianmd config=recommended

this.addCommand({ id: "plain", callback() {} }); // @expect-error glass-lint rule=obsidian:ui.command message_id=detected line=any column=any
const eventName = "file-menu";
