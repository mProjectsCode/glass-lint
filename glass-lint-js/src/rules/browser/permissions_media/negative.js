// @case description negative fixture for browser:browser.permissions-media
// @tool glass-lint rules=browser:browser.permissions-media
// A locally defined navigator is not the browser global.
const navigator = { mediaDevices: { getUserMedia() {} } };
// @expect-no-error glass-lint rule=browser:browser.permissions-media message_id=detected
navigator.mediaDevices.getUserMedia({ audio: true });

// Reassignment drops a previously rooted alias.
let media = globalThis.navigator.mediaDevices;
media = { getUserMedia() {} };
// @expect-no-error glass-lint rule=browser:browser.permissions-media message_id=detected
media.getUserMedia({ video: true });
