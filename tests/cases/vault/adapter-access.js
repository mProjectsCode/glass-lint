// @case description Adapter access is detected by reference and later operations
// @tool glass-lint rules=obsidian:vault.adapter
// @tool eslint-obsidianmd config=recommended

const adapter = this.app.vault.adapter; // @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
await adapter.someFutureMethod("daily.md");

function captureAdapter() {
  const adapter = this.app.vault.adapter; // @expect-error glass-lint rule=obsidian:vault.adapter message_id=detected
}
function useUnrelated() {
  const adapter = localStorage;
  adapter.read("daily.md");
}
