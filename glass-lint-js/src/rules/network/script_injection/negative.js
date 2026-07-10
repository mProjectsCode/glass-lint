// @case description negative fixture for js:dynamic-code.script-injection
// @tool glass-lint rules=js:dynamic-code.script-injection
// @expect-no-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("div");
