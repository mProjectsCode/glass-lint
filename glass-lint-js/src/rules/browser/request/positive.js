// @case description positive fixture for browser:network.request
// @tool glass-lint rules=browser:network.request

// Direct global fetch and a rooted browser member are detected.
// @expect-error glass-lint rule=browser:network.request
fetch("https://example.com");
// Standard global-object access resolves to the same global callable.
// @expect-error glass-lint rule=browser:network.request
window.fetch("https://window.example");
const fetchArgs = ["https://apply.example"];
// @expect-error glass-lint rule=browser:network.request
self.fetch.apply(null, fetchArgs);
const beacon = navigator;
// @expect-error glass-lint rule=browser:network.request
beacon.sendBeacon("https://example.com", "{}");
// @expect-error glass-lint rule=browser:network.request
window.navigator.sendBeacon("https://window-beacon.example", payload);
// The canonical rooted declaration also covers the standard global-object alias.
// @expect-error glass-lint rule=browser:network.request
globalThis.navigator.sendBeacon("https://global-beacon.example", payload);

// Global and rooted aliases retain provenance.
const request = fetch;
// @expect-error glass-lint rule=browser:network.request
request("/aliased");
const directBeacon = navigator.sendBeacon;
// @expect-error glass-lint rule=browser:network.request
directBeacon("https://alias.example", "{}");

// Each configured browser constructor is detected directly.
// @expect-error glass-lint rule=browser:network.request
new XMLHttpRequest();
// @expect-error glass-lint rule=browser:network.request
new WebSocket("wss://example.com");
// @expect-error glass-lint rule=browser:network.request
new EventSource("https://example.com/events");

// Constructor aliases retain global provenance.
const Xhr = XMLHttpRequest;
// @expect-error glass-lint rule=browser:network.request
new Xhr();
const Socket = WebSocket;
// @expect-error glass-lint rule=browser:network.request
new Socket("wss://alias.example");
const Events = EventSource;
// @expect-error glass-lint rule=browser:network.request
new Events("https://alias.example/events");

// Callee detection is independent of static versus dynamic request values.
const url = getUrl();
// @expect-error glass-lint rule=browser:network.request
fetch(url);
