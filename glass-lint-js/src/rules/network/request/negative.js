// @case description negative fixture for js:network.request
// @tool glass-lint rules=js:network.request
// @expect-no-error glass-lint rule=js:network.request message_id=detected
function localLookalike() { return null; }
localLookalike();
