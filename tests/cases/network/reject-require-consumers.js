// @case description Arbitrary require consumers do not become module namespaces
// @tool glass-lint rules=obsidian:network.request
// @tool eslint-obsidianmd config=recommended

const fallback = chooseFallback(require("obsidian"));
fallback.requestUrl("https://example.com");
