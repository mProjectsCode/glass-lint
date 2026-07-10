// @case description positive fixture for obsidian:vault.read
// @tool glass-lint rules=obsidian:vault.read

// @expect-error glass-lint rule=obsidian:vault.read message_id=detected
app.vault.read(file);

// @expect-error glass-lint rule=obsidian:vault.read message_id=detected
app.vault.cachedRead(file);

const v1 = app.vault;
// @expect-error glass-lint rule=obsidian:vault.read message_id=detected
v1.read(file);

let v2;
v2 = this.app.vault;
// @expect-error glass-lint rule=obsidian:vault.read message_id=detected
v2.read(file);

function readFrom(vault) {
  return vault.read(file); // @expect-error glass-lint rule=obsidian:vault.read message_id=detected
}
readFrom(this.app.vault);
