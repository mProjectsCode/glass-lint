// @case description positive fixture for browser:browser.permissions-geolocation
// @tool glass-lint rules=browser:browser.permissions-geolocation
// @expect-error glass-lint rule=browser:browser.permissions-geolocation message_id=detected
navigator.geolocation.getCurrentPosition(()=>{});
// Qualified browser-global navigator paths retain identity.
// @expect-error glass-lint rule=browser:browser.permissions-geolocation message_id=detected
window.navigator.geolocation.getCurrentPosition(() => {});
// @expect-error glass-lint rule=browser:browser.permissions-geolocation message_id=detected
self.navigator.geolocation.watchPosition(() => {});
// @expect-error glass-lint rule=browser:browser.permissions-geolocation message_id=detected
globalThis.navigator.geolocation.getCurrentPosition(() => {});
// Aliases of the rooted geolocation namespace retain provenance.
// @expect-error glass-lint rule=browser:browser.permissions-geolocation message_id=detected
navigator.geolocation.getCurrentPosition(() => {});
const geolocation = navigator.geolocation;
// @expect-error glass-lint rule=browser:browser.permissions-geolocation message_id=detected
geolocation.getCurrentPosition(() => {});
// @expect-error glass-lint rule=browser:browser.permissions-geolocation message_id=detected
navigator.geolocation.watchPosition(() => {});
