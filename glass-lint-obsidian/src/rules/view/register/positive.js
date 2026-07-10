// @case description positive fixture for obsidian:view.register
// @tool glass-lint rules=obsidian:view.register

// @expect-error glass-lint rule=obsidian:view.register message_id=detected
this.registerView('x', v1);

// @expect-error glass-lint rule=obsidian:view.register message_id=detected
this.registerView("second", view);
