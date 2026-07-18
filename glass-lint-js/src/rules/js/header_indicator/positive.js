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
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const lowerAuth = "authorization";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const cookie = "Cookie";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const setCookie = "Set-Cookie";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const proxyAuth = "Proxy-Authorization";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const apiKey = "X-API-Key";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const lowerApiKey = "x-api-key";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const authToken = "X-Auth-Token";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const accessToken = "x-access-token";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const clientToken = "X-Client-Token";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const apiToken = "x-api-token";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const upperAuth = "AUTHORIZATION";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const upperAgent = "USER-AGENT";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const upperCookie = "COOKIE";
// @expect-error glass-lint rule=js:network.header-indicator message_id=detected
const upperApiKey = "API-KEY";
