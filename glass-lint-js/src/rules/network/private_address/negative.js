// @case description negative fixture for js:network.private-address
// @tool glass-lint rules=js:network.private-address
// @expect-no-error glass-lint rule=js:network.private-address message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=js:network.private-address message_id=detected
const publicAddress = "https://example.com";
// Migrated: network/comments-and-identifiers-ignored.js
const version = "10.4.2";
const partialPrivateRange = "172.20.1";
const partialPrivatePrefix = "192.168.";
