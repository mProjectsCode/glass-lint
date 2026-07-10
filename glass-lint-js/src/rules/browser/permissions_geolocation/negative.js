// @case description negative fixture for js:browser.permissions-geolocation
// @tool glass-lint rules=js:browser.permissions-geolocation
// @expect-no-error glass-lint rule=js:browser.permissions-geolocation message_id=detected
// A locally defined navigator is not the browser global.
const navigator = { geolocation: { getCurrentPosition() {} } };
// @expect-no-error glass-lint rule=js:browser.permissions-geolocation message_id=detected
navigator.geolocation.getCurrentPosition(() => {});

// Reassignment drops the rooted namespace alias.
let geolocation = globalThis.navigator.geolocation;
geolocation = { getCurrentPosition() {} };
// @expect-no-error glass-lint rule=js:browser.permissions-geolocation message_id=detected
geolocation.getCurrentPosition(() => {});
