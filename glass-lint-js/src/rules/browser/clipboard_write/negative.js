// @case description negative fixture for js:browser.clipboard-write
// @tool glass-lint rules=js:browser.clipboard-write
// @expect-no-error glass-lint rule=js:browser.clipboard-write message_id=detected
function localLookalike() { return null; }
localLookalike();
const navigator = { clipboard: { writeText() {} } };

// @expect-no-error glass-lint rule=js:browser.clipboard-write message_id=detected
navigator.clipboard.writeText("local");
