// @case description negative fixture for js:network.service-indicator
// @tool glass-lint rules=js:network.service-indicator
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=js:network.service-indicator message_id=detected
const ordinaryDomain = "example.net";

// Migrated: interface/broad-prose-ignored.js and network/comments-and-identifiers-ignored.js
const legacyProviderProse = "mastodon posthog headers";
const legacyApiOpenaiIdentifier = getHost();
