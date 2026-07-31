// @case description negative fixture for js:network.header-indicator
// @tool glass-lint rules=js:network.header-indicator
// Unconfigured names are not marker matches.
// @expect-no-error glass-lint rule=js:network.header-indicator
const ordinaryHeader = "Content-Type";

// Dynamic compositions remain unknown to the literal matcher.
const prefix = "Auth";
// @expect-no-error glass-lint rule=js:network.header-indicator
const computedHeader = prefix + getHeaderSuffix();

// Unrelated prose without a configured marker is ignored.
const headerProse = "mastodon posthog headers";

// Same-named local helpers are irrelevant to a literal-only matcher.
function localLookalike() { return null; }
// @expect-no-error glass-lint rule=js:network.header-indicator
localLookalike();
