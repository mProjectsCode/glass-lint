// @case description Dynamic-code flow respects shadowing, reassignment, callback values, and ordering
// @tool glass-lint rules=js:dynamic-code.eval
// @tool eslint-obsidianmd config=recommended

function localOnly(eval, Function, setTimeout) {
  eval("text");
  Function("text");
  Function.constructor("text");
  setTimeout("text", 0);
}
let run = globalThis.eval;
run = safeParser;
run("text");
setTimeout(() => runCode(), 0);

const script = document.createElement("script");
script.src = "https://cdn.example.com/plugin.js";

function configure() {
  const siblingScript = document.createElement("script");
  siblingScript.src = "https://cdn.example.com/plugin.js";
}
function appendUnrelated() {
  const siblingScript = document.createElement("div");
  document.head.appendChild(siblingScript);
}

let orderedScript = document.createElement("script");
orderedScript.src = "https://cdn.example.com/plugin.js";
orderedScript = document.createElement("div");
document.head.appendChild(orderedScript);

const futureScript = document.createElement("script");
document.head.appendChild(futureScript);
futureScript.src = "https://cdn.example.com/plugin.js";
