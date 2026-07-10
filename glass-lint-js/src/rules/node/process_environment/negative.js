// @case description negative fixture for js:node.process-environment
// @tool glass-lint rules=js:node.process-environment
// @expect-no-error glass-lint rule=js:node.process-environment message_id=detected
function localLookalike() { return null; }
localLookalike();
