// @case description positive fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
eval("code");
