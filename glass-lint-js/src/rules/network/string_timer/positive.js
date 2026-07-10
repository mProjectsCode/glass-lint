// @case description positive fixture for js:dynamic-code.string-timer
// @tool glass-lint rules=js:dynamic-code.string-timer
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setTimeout("code()", 1);
// Static template strings also satisfy the string argument constraint.
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
window.setInterval(`runCode()`, 1000);
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
globalThis.setTimeout("run()", 10);
// Global aliases retain provenance.
const schedule = setTimeout;
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
schedule("later()", 0);
