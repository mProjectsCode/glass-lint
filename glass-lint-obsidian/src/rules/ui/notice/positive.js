// @case description positive fixture for obsidian:ui.notice
// @tool glass-lint rules=obsidian:ui.notice
// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
new Notice('x');
// second independent example
// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
new Notice("second");
