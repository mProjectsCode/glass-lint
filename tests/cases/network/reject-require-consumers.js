// @case description Ported old classifier case: arbitrary require consumers do not become module namespaces
// @tool glass-lint rules=obsidian:network.obsidian

const fallback = chooseFallback(require("obsidian"));
fallback.requestUrl("https://example.com");
