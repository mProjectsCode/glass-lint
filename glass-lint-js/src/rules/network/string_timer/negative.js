// @case description negative fixture for js:dynamic-code.string-timer
// @tool glass-lint rules=js:dynamic-code.string-timer
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setTimeout(() => {}, 10);
