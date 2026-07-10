// @case description String timer callbacks are dynamic code
// @case tags timers,dynamic-code
// @tool glass-lint rules=js:dynamic-code.string-timer
// @tool eslint-obsidianmd config=recommended

globalThis.setTimeout('run()', 10); // @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected severity=warning line=any column=any
