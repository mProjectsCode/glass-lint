// @case description positive fixture for js:dynamic-code.string-timer
// @tool glass-lint rules=js:dynamic-code.string-timer
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setTimeout("code()", 1);
// second independent example
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setTimeout("second()", 1);
