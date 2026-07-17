// @case description positive fixture for js:dynamic-code.string-timer
// @tool glass-lint rules=js:dynamic-code.string-timer
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setTimeout("code()", 1);
// Static template strings also satisfy the string argument constraint.
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
window.setInterval(`runCode()`, 1000);
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
globalThis.setTimeout("run()", 10);
// Bare and other proven global-object forms share callable identity.
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setInterval("repeat()", 10);
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
self.setTimeout.call(null, "callLater()", 0);
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
global.setInterval.apply(null, ["applyInterval()", 0]);
// Global aliases retain provenance.
const schedule = setTimeout;
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
schedule("later()", 0);
