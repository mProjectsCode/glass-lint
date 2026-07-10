// @case description negative fixture for js:network.url-construction
// @tool glass-lint rules=js:network.url-construction
// @expect-no-error glass-lint rule=js:network.url-construction message_id=detected
function localLookalike() { return null; }
localLookalike();
