// @case description positive fixture for browser:dynamic-code.script-injection
// @tool glass-lint rules=browser:dynamic-code.script-injection
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
document.createElement("script");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
window.document.createElement("script");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
globalThis.document.createElement("script");
// Insertion and content configuration are not necessary for this creation rule.
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
const inlineScript = document.createElement("script");
inlineScript.textContent = generatedCode;

// Static HTML sinks with executable markers are also classified.
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
document.write("<script src='https://example.test/app.js'></script>");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
window.document.write("<script>windowed</script>");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
document.writeln("javascript:alert(1)");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
window.document.writeln("javascript:windowed()");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
globalThis.document.writeln("<script>global</script>");
