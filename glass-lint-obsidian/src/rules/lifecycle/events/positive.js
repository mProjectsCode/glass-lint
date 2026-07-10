// @case description positive fixture for obsidian:lifecycle.events
// @tool glass-lint rules=obsidian:lifecycle.events
// @expect-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this.registerEvent(app.vault.on('changed',fn));
// second independent example

// @expect-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this.registerInterval(setInterval(() => {}, 1000));
