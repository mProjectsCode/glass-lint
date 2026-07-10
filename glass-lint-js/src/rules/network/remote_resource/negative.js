// @case description negative fixture for js:dom.remote-resource
// @tool glass-lint rules=js:dom.remote-resource
// @expect-no-error glass-lint rule=js:dom.remote-resource message_id=detected
function localLookalike() { return null; }
localLookalike();
