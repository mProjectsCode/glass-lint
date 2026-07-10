// @case description positive fixture for obsidian:vault.adapter
// @tool glass-lint rules=obsidian:vault.adapter

// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
app.vault.adapter;
// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
app.vault.adapter.exists(path);
// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
const a = this.app.vault.adapter;
await a.someFutureMethod("daily.md");
function captureAdapter() {
  // @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
  const a = this.app.vault.adapter;
}
