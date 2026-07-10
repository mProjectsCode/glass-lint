// @case description positive fixture for js:browser.permissions-bluetooth
// @tool glass-lint rules=js:browser.permissions-bluetooth
// @expect-error glass-lint rule=js:browser.permissions-bluetooth message_id=detected
navigator.bluetooth.requestDevice({});
// second independent example

// @expect-error glass-lint rule=js:browser.permissions-bluetooth message_id=detected
navigator.bluetooth.requestDevice({ filters: [] });
const bluetooth = navigator.bluetooth;

// @expect-error glass-lint rule=js:browser.permissions-bluetooth message_id=detected
bluetooth.requestDevice({});
