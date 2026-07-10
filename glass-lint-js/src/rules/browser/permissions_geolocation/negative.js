// @case description negative fixture for js:browser.permissions-geolocation
// @tool glass-lint rules=js:browser.permissions-geolocation
// @expect-no-error glass-lint rule=js:browser.permissions-geolocation message_id=detected
function localLookalike() { return null; }
localLookalike();
