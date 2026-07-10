// @case description negative fixture for js:browser.environment
// @tool glass-lint rules=js:browser.environment
// @expect-no-error glass-lint rule=js:browser.environment message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=js:browser.environment message_id=detected
screen.colorDepth;
