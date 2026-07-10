// @case description positive fixture for js:dynamic-code.script-injection
// @tool glass-lint rules=js:dynamic-code.script-injection
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("script");
// Migrated: system/dynamic-code-dom-injection.js

// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
const remoteScript = document.createElement("script");
remoteScript.src = "https://cdn.example.com/plugin.js";
document.head.appendChild(remoteScript);
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
const inlineScript = document.createElement("script");
inlineScript.textContent = generatedCode;
document.body.prepend(inlineScript);
// Migrated: system/dynamic-code-helper-flow.js
function appendToHead(node) { document.head.appendChild(node); }

// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
const helperScript = document.createElement("script");
helperScript.src = "https://cdn.example.com/plugin.js";
appendToHead(helperScript);

// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("script");
// second independent example
document.createElement("script");
