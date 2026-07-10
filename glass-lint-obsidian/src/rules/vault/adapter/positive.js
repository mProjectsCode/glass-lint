// @case description positive fixture for obsidian:vault.adapter
// @tool glass-lint rules=obsidian:vault.adapter

// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
app.vault.adapter;

// @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
app.vault.adapter.exists(path);

const a = this.app.vault.adapter; // @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
await a.someFutureMethod("daily.md");

function captureAdapter() {
  const a = this.app.vault.adapter; // @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
}
