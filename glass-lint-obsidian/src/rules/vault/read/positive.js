// @case description all configured read methods, aliases, and bounded flow
// @tool glass-lint rules=obsidian:vault.read

// @expect-error glass-lint rule=obsidian:vault.read
app.vault.read(file);
// @expect-error glass-lint rule=obsidian:vault.read
app.vault.cachedRead(file);
// @expect-error glass-lint rule=obsidian:vault.read
app.vault.readBinary(file);

const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.read
vault.read(file);
// @expect-error glass-lint rule=obsidian:vault.read
this["app"]["vault"]["cachedRead"](file);

function readFrom(vault) {
  // @expect-error glass-lint rule=obsidian:vault.read
  return vault.readBinary(file);
}
readFrom(this.app.vault);

// @expect-error glass-lint rule=obsidian:vault.read
this.app.vault.read(file);
