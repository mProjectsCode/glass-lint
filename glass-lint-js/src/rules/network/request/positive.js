// @case description positive fixture for js:network.request
// @tool glass-lint rules=js:network.request
// @expect-error glass-lint rule=js:network.request message_id=detected
fetch("https://example.com");
// second independent example

// @expect-error glass-lint rule=js:network.request message_id=detected
fetch("/second");
const request = fetch;

// @expect-error glass-lint rule=js:network.request message_id=detected
request("/aliased");
// Migrated: network/common-apis.js

// @expect-error glass-lint rule=js:network.request message_id=detected
navigator.sendBeacon("https://example.com", "{}");

// @expect-error glass-lint rule=js:network.request message_id=detected
new XMLHttpRequest();

// @expect-error glass-lint rule=js:network.request message_id=detected
new WebSocket("wss://example.com");

// @expect-error glass-lint rule=js:network.request message_id=detected
new EventSource("https://example.com/events");
// Migrated: network/multiple-fetch.js

// @expect-error glass-lint rule=js:network.request message_id=detected
fetch("/legacy-one");

// @expect-error glass-lint rule=js:network.request message_id=detected
fetch("/legacy-two");
