// @case description negative fixture for browser:network.request
// @tool glass-lint rules=browser:network.request

// A local fetch binding shadows the browser global.
function localFetch(fetch) {
  // @expect-no-error glass-lint rule=browser:network.request
  fetch("/local");
}
localFetch(() => {});

function localWindow(window) {
  // @expect-no-error glass-lint rule=browser:network.request
  window.fetch("/local-window");
}
localWindow({ fetch() {} });
globalThis.fetch = localFetch;
// @expect-no-error glass-lint rule=browser:network.request
globalThis.fetch("/mutated-global");

// Local lookalike functions and constructors are not browser APIs.
function localLookalike() { return null; }
// @expect-no-error glass-lint rule=browser:network.request
localLookalike();
const WebSocket = function LocalSocket() {};
// @expect-no-error glass-lint rule=browser:network.request
new WebSocket("not-network");

// Reassignment drops provenance from global and rooted aliases.
let reassignedFetch = fetch;
reassignedFetch = localFetch;
// @expect-no-error glass-lint rule=browser:network.request
reassignedFetch("not-network");
let reassignedBeacon = navigator;
reassignedBeacon = {};
// @expect-no-error glass-lint rule=browser:network.request
reassignedBeacon.sendBeacon("not-network", "{}");

let reassignedConstructor = XMLHttpRequest;
reassignedConstructor = LocalConstructor;
// @expect-no-error glass-lint rule=browser:network.request
new reassignedConstructor();

function LocalConstructor() {}

// A local navigator object does not establish rooted browser provenance.
function localNavigatorCase() {
  const navigator = { sendBeacon() {} };
  // @expect-no-error glass-lint rule=browser:network.request
  navigator.sendBeacon("local", "{}");
}
localNavigatorCase();

function localWindowNavigator(window) {
  // @expect-no-error glass-lint rule=browser:network.request
  window.navigator.sendBeacon("local", "{}");
}
localWindowNavigator({ navigator: { sendBeacon() {} } });

// A nested scope still sees the unshadowed browser global.
function networkCall() {
  // @expect-error glass-lint rule=browser:network.request
  fetch("https://example.com");
}
networkCall();
