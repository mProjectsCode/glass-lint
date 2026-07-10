// @case description positive fixture for js:dynamic-code.string-timer
// @tool glass-lint rules=js:dynamic-code.string-timer
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setTimeout("code()", 1);
// second independent example

// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setTimeout("second()", 1);

// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
window.setInterval("intervalCode()", 10);
// Migrated: system/dynamic-code-string-timers.js and system/global-this-timer.js

// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setTimeout("runCode()", 0);

// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
window.setInterval(`runCode()`, 1000);
globalThis.setTimeout("run()", 10);
