// @case description all configured vault write APIs, rooted aliases, and static properties
// @tool glass-lint rules=obsidian:vault.write

// Direct calls cover every configured method.
// @expect-error glass-lint rule=obsidian:vault.write
app.vault.create(file, text);
// @expect-error glass-lint rule=obsidian:vault.write
app.vault.createBinary(file, data);
// @expect-error glass-lint rule=obsidian:vault.write
app.vault.modify(file, text);
// @expect-error glass-lint rule=obsidian:vault.write
app.vault.modifyBinary(file, data);
// @expect-error glass-lint rule=obsidian:vault.write
app.vault.append(file, text);
// @expect-error glass-lint rule=obsidian:vault.write
app.vault.appendBinary(file, data);
// @expect-error glass-lint rule=obsidian:vault.write
app.vault.process(file, processor);
// @expect-error glass-lint rule=obsidian:vault.write
app.vault.createFolder(path);

// `this.app`, receiver aliases, and static computed names retain rooted provenance.
// @expect-error glass-lint rule=obsidian:vault.write
this.app.vault.createFolder("new");
const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.write
vault.modify(file, text);
// @expect-error glass-lint rule=obsidian:vault.write
app["vault"]["appendBinary"](file, data);
