// @case description positive fixture for js:network.header-indicator
// @tool glass-lint rules=js:network.header-indicator
// Configured header marker substrings are detected in ordinary literals.
// @expect-error glass-lint rule=js:network.header-indicator
const authHeader = "Authorization";
// @expect-error glass-lint rule=js:network.header-indicator
const agentHeader = "user-agent";
// @expect-error glass-lint rule=js:network.header-indicator
const browserHeader = "User-Agent";

// The heuristic does not require request API context.
// @expect-error glass-lint rule=js:network.header-indicator
const x = "X-Authorization-Token";
// @expect-error glass-lint rule=js:network.header-indicator
const lowerAuth = "authorization";
// @expect-error glass-lint rule=js:network.header-indicator
const cookie = "Cookie";
// @expect-error glass-lint rule=js:network.header-indicator
const setCookie = "Set-Cookie";
// @expect-error glass-lint rule=js:network.header-indicator
const proxyAuth = "Proxy-Authorization";
// @expect-error glass-lint rule=js:network.header-indicator
const apiKey = "X-API-Key";
// @expect-error glass-lint rule=js:network.header-indicator
const lowerApiKey = "x-api-key";
// @expect-error glass-lint rule=js:network.header-indicator
const authToken = "X-Auth-Token";
// @expect-error glass-lint rule=js:network.header-indicator
const accessToken = "x-access-token";
// @expect-error glass-lint rule=js:network.header-indicator
const clientToken = "X-Client-Token";
// @expect-error glass-lint rule=js:network.header-indicator
const apiToken = "x-api-token";
// @expect-error glass-lint rule=js:network.header-indicator
const upperAuth = "AUTHORIZATION";
// @expect-error glass-lint rule=js:network.header-indicator
const upperAgent = "USER-AGENT";
// @expect-error glass-lint rule=js:network.header-indicator
const upperCookie = "COOKIE";
// @expect-error glass-lint rule=js:network.header-indicator
const upperApiKey = "API-KEY";
