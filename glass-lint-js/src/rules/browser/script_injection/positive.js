// @case description positive fixture for browser:dynamic-code.script-injection
// @tool glass-lint rules=browser:dynamic-code.script-injection
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
document.createElement("script");
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
window.document.createElement("script");
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
globalThis.document.createElement("script");
// Configuration and insertion complete the executable object flow.
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
const inlineScript = document.createElement("script");
inlineScript.src = "https://example.test/app.js";
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
document.head.appendChild(inlineScript);

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
