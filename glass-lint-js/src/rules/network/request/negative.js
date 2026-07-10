// @case description negative fixture for js:network.request
// @tool glass-lint rules=js:network.request
// @expect-no-error glass-lint rule=js:network.request message_id=detected
function localLookalike() { return null; }
localLookalike();
function fetch() {}
// @expect-no-error glass-lint rule=js:network.request message_id=detected
fetch("/local");

// Migrated: network/compact-local-lookalikes.js and network/local-lookalikes.js
function legacyLocalFetch(value) {
  function fetch(value) { return value; }
  return fetch(value);
}
legacyLocalFetch("not-network");

// Migrated: network/import-and-global-shadowing.js
function legacyLocalFetch(fetch) {
  fetch("not-network"); // @expect-no-error glass-lint rule=js:network.request message_id=detected
}

// Migrated: network/shadowing-sibling-scopes.js
function legacyNetworkCall() {
  fetch("https://example.com");
}
legacyNetworkCall();
