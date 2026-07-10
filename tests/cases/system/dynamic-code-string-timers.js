// @case description String timer callbacks are detected as dynamic code
// @tool glass-lint rules=js:dynamic-code.string-timer
// @tool eslint-obsidianmd config=recommended

setTimeout("runCode()", 0); // @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
window.setInterval(`runCode()`, 1000); // @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
