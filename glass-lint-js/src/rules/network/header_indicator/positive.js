// @case description positive fixture for js:network.header-indicator
// @tool glass-lint rules=js:network.header-indicator
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const x="Authorization";
// second independent example

// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const authHeader = "Authorization";

// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const agentHeader = "user-agent";
