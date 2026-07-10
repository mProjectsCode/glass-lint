// @case description positive fixture for obsidian:ui.modal
// @tool glass-lint rules=obsidian:ui.modal
// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
new Modal(app);
// second independent example
// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
new Modal(app);
