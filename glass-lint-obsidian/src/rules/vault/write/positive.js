// @case description positive fixture for obsidian:vault.write
// @tool glass-lint rules=obsidian:vault.write

// @expect-error glass-lint rule=obsidian:vault.write message_id=detected
app.vault.modify(file, text);

// @expect-error glass-lint rule=obsidian:vault.write message_id=detected
app.vault.append(file, text);

const w1 = app.vault;
// @expect-error glass-lint rule=obsidian:vault.write message_id=detected
w1.modify(file, text);

// @expect-error glass-lint rule=obsidian:vault.write message_id=detected
this.app.vault.createFolder("new");

// @expect-error glass-lint rule=obsidian:vault.write message_id=detected
this.app.vault.appendBinary(file, data);

const { vault: w2 } = this.app;
// @expect-error glass-lint rule=obsidian:vault.write message_id=detected
w2.modify(file, text);
