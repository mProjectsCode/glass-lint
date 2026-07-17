// @case description negative fixture for browser:browser.permissions-bluetooth
// @tool glass-lint rules=browser:browser.permissions-bluetooth
// @expect-no-error glass-lint rule=browser:browser.permissions-bluetooth message_id=detected
// A locally defined navigator is not the browser global.
const navigator = { bluetooth: { requestDevice() {} } };
// @expect-no-error glass-lint rule=browser:browser.permissions-bluetooth message_id=detected
navigator.bluetooth.requestDevice({});

// Reassignment drops the rooted namespace alias.
let bluetooth = globalThis.navigator.bluetooth;
bluetooth = { requestDevice() {} };
// @expect-no-error glass-lint rule=browser:browser.permissions-bluetooth message_id=detected
bluetooth.requestDevice({});
