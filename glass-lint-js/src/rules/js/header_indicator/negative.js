// @case description negative fixture for js:network.header-indicator
// @tool glass-lint rules=js:network.header-indicator
// Unconfigured names are not marker matches.
// @expect-no-error glass-lint rule=js:network.header-indicator message_id=detected
const ordinaryHeader = "Content-Type";

// A computed or concatenated value is not reconstructed by this literal rule.
const prefix = "Auth";
// @expect-no-error glass-lint rule=js:network.header-indicator message_id=detected
const computedHeader = prefix + "orization";

// Unrelated prose without a configured marker is ignored.
const headerProse = "mastodon posthog headers";

// Same-named local helpers are irrelevant to a literal-only matcher.
function localLookalike() { return null; }
// @expect-no-error glass-lint rule=js:network.header-indicator message_id=detected
localLookalike();
