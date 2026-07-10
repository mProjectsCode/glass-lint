// @case description positive fixture for js:dynamic-code.script-injection
// @tool glass-lint rules=js:dynamic-code.script-injection
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("script");
// second independent example
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("script");
