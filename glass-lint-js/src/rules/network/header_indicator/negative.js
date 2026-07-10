// @case description negative fixture for js:network.header-indicator
// @tool glass-lint rules=js:network.header-indicator
// @expect-no-error glass-lint rule=js:network.header-indicator message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=js:network.header-indicator message_id=detected
const ordinaryHeader = "Content-Type";
// Migrated: interface/broad-prose-ignored.js
const headerProse = "mastodon posthog headers";
