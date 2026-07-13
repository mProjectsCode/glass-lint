// @case description positive fixture for js:dynamic-code.script-injection
// @tool glass-lint rules=js:dynamic-code.script-injection
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("script");
// Insertion and content configuration are not necessary for this creation rule.
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
const inlineScript = document.createElement("script");
inlineScript.textContent = generatedCode;
