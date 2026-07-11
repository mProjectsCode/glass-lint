// @case description local lookalikes, shadowing, dynamic and unlisted methods, and reassignment
// @tool glass-lint rules=obsidian:vault.write

const localApp = { vault: { modify() {} } };
// @expect-no-error glass-lint rule=obsidian:vault.write message_id=detected
localApp.vault.modify(file, text);

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:vault.write message_id=detected
  app.vault.append(file, text);
}
shadowed({ vault: { append() {} } });

// Dynamic and unlisted members are outside the configured method set.
// @expect-no-error glass-lint rule=obsidian:vault.write message_id=detected
app.vault[method](file, text);
// @expect-no-error glass-lint rule=obsidian:vault.write message_id=detected
app.vault.rename(file, name);

// A receiver alias is valid only before it is reassigned.
let vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.write message_id=detected
vault.modify(file, text);
vault = localVault;
// @expect-no-error glass-lint rule=obsidian:vault.write message_id=detected
vault.modify(file, text);
