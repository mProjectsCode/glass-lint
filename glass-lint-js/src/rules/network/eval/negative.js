// @case description negative fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval
// @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
function localLookalike() { return null; }
localLookalike();
