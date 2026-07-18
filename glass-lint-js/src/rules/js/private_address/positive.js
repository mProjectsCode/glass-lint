// @case description positive fixture for js:network.private-address
// @tool glass-lint rules=js:network.private-address
// Each configured marker is detected inside a string literal.
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const localhost = "localhost";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const loopback = "http://127.0.0.1";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const wildcard = "0.0.0.0";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const httpLan = "http://192.168.1.2";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const httpsLan = "https://192.168.1.2";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const httpTen = "http://10.0.0.2";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const httpsTen = "https://10.0.0.2";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const httpPrivate172 = "http://172.16.4.2";
// The full RFC 1918 172.16.0.0/12 URL range is covered.
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const httpsPrivate172 = "https://172.31.255.254";
// URL loopback prefixes are covered beyond the single localhost address.
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const urlLoopbackRange = "http://127.42.1.1";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const httpsLinkLocal = "https://169.254.1.2";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const ipv6Loopback = "http://[::1]";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const ipv6UniqueLocal = "http://[fd12::1]";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const ipv6LinkLocal = "http://[fe80::1]";
