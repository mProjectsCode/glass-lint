// @case description negative fixture for js:network.private-address
// @tool glass-lint rules=js:network.private-address
// @expect-no-error glass-lint rule=js:network.private-address message_id=detected
function localLookalike() { return null; }
localLookalike();
