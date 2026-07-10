// @case description Remote DOM flow follows arguments into direct helpers
// @tool glass-lint rules=js:dynamic-code.script-injection
// @tool eslint-obsidianmd config=recommended

function appendToHead(node) {
  document.head.appendChild(node);
}
const script = document.createElement("script"); // @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
script.src = "https://cdn.example.com/plugin.js";
appendToHead(script);
