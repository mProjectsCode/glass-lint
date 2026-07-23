// @case description negative fixture for browser:browser.permissions-bluetooth
// @tool glass-lint rules=browser:browser.permissions-bluetooth
// @expect-no-error glass-lint rule=browser:browser.permissions-bluetooth
// A locally defined navigator is not the browser global.
const navigator = { bluetooth: { requestDevice() {} } };
// @expect-no-error glass-lint rule=browser:browser.permissions-bluetooth
navigator.bluetooth.requestDevice({});

// Reassignment drops the rooted namespace alias.
let bluetooth = globalThis.navigator.bluetooth;
bluetooth = { requestDevice() {} };
// @expect-no-error glass-lint rule=browser:browser.permissions-bluetooth
bluetooth.requestDevice({});

// Dynamic properties cannot establish the requested permission API.
// @expect-no-error glass-lint rule=browser:browser.permissions-bluetooth
navigator.bluetooth[method]({});

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.permissions-bluetooth
    window.navigator.bluetooth.requestDevice({});
}
localWindow({ navigator: { bluetooth: { requestDevice() {} } } });
