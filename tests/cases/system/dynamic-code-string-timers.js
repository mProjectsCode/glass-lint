// @case description String timer callbacks are detected as dynamic code
// @tool glass-lint rules=obsidian:dynamic_code
// @tool eslint-obsidianmd config=recommended

setTimeout("runCode()", 0); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
window.setInterval(`runCode()`, 1000); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
