// @case description positive fixture for browser:browser.permissions-bluetooth
// @tool glass-lint rules=browser:browser.permissions-bluetooth
// @expect-error glass-lint rule=browser:browser.permissions-bluetooth
navigator.bluetooth.requestDevice({});
// @expect-error glass-lint rule=browser:browser.permissions-bluetooth
window.navigator.bluetooth.requestDevice({});
// @expect-error glass-lint rule=browser:browser.permissions-bluetooth
self.navigator.bluetooth.requestDevice({});
// @expect-error glass-lint rule=browser:browser.permissions-bluetooth
globalThis.navigator.bluetooth.requestDevice({});
// Aliases of the rooted Bluetooth namespace retain provenance.
// @expect-error glass-lint rule=browser:browser.permissions-bluetooth
navigator.bluetooth.requestDevice({ filters: [] });
const bluetooth = navigator.bluetooth;
// @expect-error glass-lint rule=browser:browser.permissions-bluetooth
bluetooth.requestDevice({});
// Static computed properties preserve rooted Bluetooth provenance.
// @expect-error glass-lint rule=browser:browser.permissions-bluetooth
navigator["bluetooth"]["requestDevice"]({});
