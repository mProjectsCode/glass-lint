// @case description Unselected network rule produces no diagnostics
// @tool glass-lint rules=obsidian:vault.read
// @tool eslint-obsidianmd config=recommended

fetch('/api/data');
