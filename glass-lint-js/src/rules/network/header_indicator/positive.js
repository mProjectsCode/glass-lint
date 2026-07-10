// @case description positive fixture for js:network.header-indicator
// @tool glass-lint rules=js:network.header-indicator
// Configured header marker substrings are detected in ordinary literals.
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const authHeader = "Authorization";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const agentHeader = "user-agent";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const browserHeader = "User-Agent";

// The heuristic does not require request API context.
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const x = "X-Authorization-Token";
