// @case description negative fixture for js:network.request
// @tool glass-lint rules=js:network.request

// A local fetch binding shadows the browser global.
function localFetch(fetch) {
  // @expect-no-error glass-lint rule=js:network.request message_id=detected
  fetch("/local");
}
localFetch(() => {});

function localWindow(window) {
  // @expect-no-error glass-lint rule=js:network.request message_id=detected
  window.fetch("/local-window");
}
localWindow({ fetch() {} });
globalThis.fetch = localFetch;
// @expect-no-error glass-lint rule=js:network.request message_id=detected
globalThis.fetch("/mutated-global");

// Local lookalike functions and constructors are not browser APIs.
function localLookalike() { return null; }
// @expect-no-error glass-lint rule=js:network.request message_id=detected
localLookalike();
const WebSocket = function LocalSocket() {};
// @expect-no-error glass-lint rule=js:network.request message_id=detected
new WebSocket("not-network");

// Reassignment drops provenance from global and rooted aliases.
let reassignedFetch = fetch;
reassignedFetch = localFetch;
// @expect-no-error glass-lint rule=js:network.request message_id=detected
reassignedFetch("not-network");
let reassignedBeacon = navigator;
reassignedBeacon = {};
// @expect-no-error glass-lint rule=js:network.request message_id=detected
reassignedBeacon.sendBeacon("not-network", "{}");

let reassignedConstructor = XMLHttpRequest;
reassignedConstructor = LocalConstructor;
// @expect-no-error glass-lint rule=js:network.request message_id=detected
new reassignedConstructor();

function LocalConstructor() {}

// A local navigator object does not establish rooted browser provenance.
function localNavigatorCase() {
  const navigator = { sendBeacon() {} };
  // @expect-no-error glass-lint rule=js:network.request message_id=detected
  navigator.sendBeacon("local", "{}");
}
localNavigatorCase();

// A nested scope still sees the unshadowed browser global.
function networkCall() {
  // @expect-error glass-lint rule=js:network.request message_id=detected
  fetch("https://example.com");
}
networkCall();
