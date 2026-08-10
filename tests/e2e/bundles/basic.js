// @case description A bundle with no findings remains clean
// @bundle web,obsidian
// @tool glass-lint rules=obsidian:network.request

var value = 1;
globalThis.value = value;
