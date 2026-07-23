// @case description positive fixture for browser:dynamic-code.script-injection
// @tool glass-lint rules=browser:dynamic-code.script-injection
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
document.createElement("script");
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
window.document.createElement("script");
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
globalThis.document.createElement("script");
// Configuration and insertion complete the executable object flow.
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
const inlineScript = document.createElement("script");
inlineScript.src = "https://example.test/app.js";
// @expect-error glass-lint rule=browser:dynamic-code.script-injection
document.head.appendChild(inlineScript);

// Static HTML sinks with executable markers are also classified.
// @expect-error glass-lint rule=browser:dynamic-code.script-injection
document.write("<script src='https://example.test/app.js'></script>");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection
window.document.write("<script>windowed</script>");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection
document.writeln("javascript:alert(1)");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection
window.document.writeln("javascript:windowed()");
// @expect-error glass-lint rule=browser:dynamic-code.script-injection
globalThis.document.writeln("<script>global</script>");
