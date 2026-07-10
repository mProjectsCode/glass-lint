// @case description positive fixture for js:browser.permissions-geolocation
// @tool glass-lint rules=js:browser.permissions-geolocation
// @expect-error glass-lint rule=js:browser.permissions-geolocation message_id=detected
navigator.geolocation.getCurrentPosition(()=>{});
// Aliases of the rooted geolocation namespace retain provenance.
// @expect-error glass-lint rule=js:browser.permissions-geolocation message_id=detected
navigator.geolocation.getCurrentPosition(() => {});
const geolocation = navigator.geolocation;
// @expect-error glass-lint rule=js:browser.permissions-geolocation message_id=detected
geolocation.getCurrentPosition(() => {});
