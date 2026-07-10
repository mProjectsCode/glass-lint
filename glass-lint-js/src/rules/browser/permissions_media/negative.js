// @case description negative fixture for js:browser.permissions-media
// @tool glass-lint rules=js:browser.permissions-media
// @expect-no-error glass-lint rule=js:browser.permissions-media message_id=detected
function localLookalike() { return null; }
localLookalike();
const navigator = { mediaDevices: { getUserMedia() {} } };
// @expect-no-error glass-lint rule=js:browser.permissions-media message_id=detected
navigator.mediaDevices.getUserMedia({ audio: true });
