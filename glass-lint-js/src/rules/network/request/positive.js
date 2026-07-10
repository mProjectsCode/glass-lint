// @case description positive fixture for js:network.request
// @tool glass-lint rules=js:network.request

// Direct global fetch and a rooted browser member are detected.
// @expect-error glass-lint rule=js:network.request message_id=detected
fetch("https://example.com");
const beacon = navigator;
// @expect-error glass-lint rule=js:network.request message_id=detected
beacon.sendBeacon("https://example.com", "{}");

// Global and rooted aliases retain provenance.
const request = fetch;
// @expect-error glass-lint rule=js:network.request message_id=detected
request("/aliased");
const directBeacon = navigator.sendBeacon;
// @expect-error glass-lint rule=js:network.request message_id=detected
directBeacon("https://alias.example", "{}");

// Each configured browser constructor is detected directly.
// @expect-error glass-lint rule=js:network.request message_id=detected
new XMLHttpRequest();
// @expect-error glass-lint rule=js:network.request message_id=detected
new WebSocket("wss://example.com");
// @expect-error glass-lint rule=js:network.request message_id=detected
new EventSource("https://example.com/events");

// Constructor aliases retain global provenance.
const Xhr = XMLHttpRequest;
// @expect-error glass-lint rule=js:network.request message_id=detected
new Xhr();
const Socket = WebSocket;
// @expect-error glass-lint rule=js:network.request message_id=detected
new Socket("wss://alias.example");
const Events = EventSource;
// @expect-error glass-lint rule=js:network.request message_id=detected
new Events("https://alias.example/events");

// Callee detection is independent of static versus dynamic request values.
const url = getUrl();
// @expect-error glass-lint rule=js:network.request message_id=detected
fetch(url);
