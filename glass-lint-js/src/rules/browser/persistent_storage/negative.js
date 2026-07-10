// @case description negative fixture for js:browser.persistent-storage
// @tool glass-lint rules=js:browser.persistent-storage
// @expect-no-error glass-lint rule=js:browser.persistent-storage message_id=detected
function localLookalike() { return null; }
localLookalike();
