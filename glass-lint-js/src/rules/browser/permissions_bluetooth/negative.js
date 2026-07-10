// @case description negative fixture for js:browser.permissions-bluetooth
// @tool glass-lint rules=js:browser.permissions-bluetooth
// @expect-no-error glass-lint rule=js:browser.permissions-bluetooth message_id=detected
function localLookalike() { return null; }
localLookalike();
const navigator = { bluetooth: { requestDevice() {} } };

// @expect-no-error glass-lint rule=js:browser.permissions-bluetooth message_id=detected
navigator.bluetooth.requestDevice({});
