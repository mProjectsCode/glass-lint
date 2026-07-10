// @case description Rooted vault aliases, assignments, destructuring, and argument flow preserve provenance
// @tool glass-lint rules=obsidian:vault.read,obsidian:vault.write,obsidian:vault.enumerate
// @tool eslint-obsidianmd config=recommended

let late;
late = this.app.vault;
late.read(file); // @expect-error glass-lint rule=obsidian:vault.read message_id=detected

const { app: { vault: nested } } = this;
nested.modify(file, text); // @expect-error glass-lint rule=obsidian:vault.write message_id=detected

const holder = {};
holder.vault = this.app.vault;
holder.vault.getFiles(); // @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected

function readFrom(vault) {
  return vault.read(file); // @expect-error glass-lint rule=obsidian:vault.read message_id=detected
}
readFrom(this.app.vault);

const a = this.app.vault;
a.read(file); // @expect-error glass-lint rule=obsidian:vault.read message_id=detected
const { vault: v } = this.app;
v.modify(file, text); // @expect-error glass-lint rule=obsidian:vault.write message_id=detected
