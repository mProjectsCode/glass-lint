// @case description negative fixture for js:browser.clipboard-read
// @tool glass-lint rules=js:browser.clipboard-read
// @expect-no-error glass-lint rule=js:browser.clipboard-read message_id=detected
function localLookalike() { return null; }
localLookalike();
const navigator = { clipboard: { readText() {} } };

// @expect-no-error glass-lint rule=js:browser.clipboard-read message_id=detected
navigator.clipboard.readText();
