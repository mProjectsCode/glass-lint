// @case description negative fixture for js:network.private-address
// @tool glass-lint rules=js:network.private-address
// Public addresses and unconfigured private ranges are ignored.
// @expect-no-error glass-lint rule=js:network.private-address message_id=detected
const publicAddress = "https://example.com";
// @expect-no-error glass-lint rule=js:network.private-address message_id=detected
const unlistedRange = "http://172.20.1.4";
// @expect-no-error glass-lint rule=js:network.private-address message_id=detected
const missingPrefix = "192.168.1.2";

// Partial markers are not expanded into URL or IP ranges.
const version = "10.4.2";
const partialPrivateRange = "172.20.1";
const partialPrivatePrefix = "192.168.";

// Concatenated and dynamic values are not reconstructed.
const host = "10.0.0.2";
const scheme = getScheme();
// @expect-no-error glass-lint rule=js:network.private-address message_id=detected
const dynamicAddress = scheme + "://" + host;

// A same-named local helper is unrelated to literal matching.
function localLookalike() { return null; }
// @expect-no-error glass-lint rule=js:network.private-address message_id=detected
localLookalike();
