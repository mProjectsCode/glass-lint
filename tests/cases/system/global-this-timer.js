// @case description String timer callbacks are dynamic code
// @case tags timers,dynamic-code
// @tool glass-lint rules=obsidian:dynamic_code

// @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected severity=warning
globalThis.setTimeout('run()', 10);
