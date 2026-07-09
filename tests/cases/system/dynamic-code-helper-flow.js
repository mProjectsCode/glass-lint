// @case description Ported old classifier case: remote DOM flow follows arguments into direct helpers
// @tool glass-lint rules=obsidian:dynamic_code

function appendToHead(node) {
  document.head.appendChild(node);
}
const script = document.createElement("script"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
script.src = "https://cdn.example.com/plugin.js";
appendToHead(script);
