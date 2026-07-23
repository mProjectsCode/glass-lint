// @case description negative fixture for browser:browser.permissions-geolocation
// @tool glass-lint rules=browser:browser.permissions-geolocation
// @expect-no-error glass-lint rule=browser:browser.permissions-geolocation
// A locally defined navigator is not the browser global.
const navigator = { geolocation: { getCurrentPosition() {} } };
// @expect-no-error glass-lint rule=browser:browser.permissions-geolocation
navigator.geolocation.getCurrentPosition(() => {});

// Reassignment drops the rooted namespace alias.
let geolocation = globalThis.navigator.geolocation;
geolocation = { getCurrentPosition() {} };
// @expect-no-error glass-lint rule=browser:browser.permissions-geolocation
geolocation.getCurrentPosition(() => {});

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.permissions-geolocation
    window.navigator.geolocation.getCurrentPosition(() => {});
}
localWindow({ navigator: { geolocation: { getCurrentPosition() {} } } });
