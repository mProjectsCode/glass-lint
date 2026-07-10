// @case description positive fixture for js:dynamic-code.script-injection
// @tool glass-lint rules=js:dynamic-code.script-injection
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("script");

// Migrated: system/dynamic-code-dom-injection.js
const legacyRemoteScript = document.createElement("script"); // @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
legacyRemoteScript.src = "https://cdn.example.com/plugin.js";
document.head.appendChild(legacyRemoteScript);

const legacyInlineScript = document.createElement("script"); // @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
legacyInlineScript.textContent = generatedCode;
document.body.prepend(legacyInlineScript);

// Migrated: system/dynamic-code-helper-flow.js
function legacyAppendToHead(node) { document.head.appendChild(node); }
const legacyHelperScript = document.createElement("script"); // @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
legacyHelperScript.src = "https://cdn.example.com/plugin.js";
legacyAppendToHead(legacyHelperScript);
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("script");
// second independent example
document.createElement("script");
