// @case description positive fixture for js:network.private-address
// @tool glass-lint rules=js:network.private-address
// Each configured marker is detected inside a string literal.
// @expect-error glass-lint rule=js:network.private-address
const localhost = "localhost";
// @expect-error glass-lint rule=js:network.private-address
const loopback = "http://127.0.0.1";
// @expect-error glass-lint rule=js:network.private-address
const wildcard = "0.0.0.0";
// @expect-error glass-lint rule=js:network.private-address
const httpLan = "http://192.168.1.2";
// @expect-error glass-lint rule=js:network.private-address
const httpsLan = "https://192.168.1.2";
// @expect-error glass-lint rule=js:network.private-address
const httpTen = "http://10.0.0.2";
// @expect-error glass-lint rule=js:network.private-address
const httpsTen = "https://10.0.0.2";
// @expect-error glass-lint rule=js:network.private-address
const httpPrivate172 = "http://172.16.4.2";
// The full RFC 1918 172.16.0.0/12 URL range is covered.
// @expect-error glass-lint rule=js:network.private-address
const httpsPrivate172 = "https://172.31.255.254";
// URL loopback prefixes are covered beyond the single localhost address.
// @expect-error glass-lint rule=js:network.private-address
const urlLoopbackRange = "http://127.42.1.1";
// @expect-error glass-lint rule=js:network.private-address
const httpsLinkLocal = "https://169.254.1.2";
// @expect-error glass-lint rule=js:network.private-address
const ipv6Loopback = "http://[::1]";
// @expect-error glass-lint rule=js:network.private-address
const ipv6UniqueLocal = "http://[fd12::1]";
// @expect-error glass-lint rule=js:network.private-address
const ipv6LinkLocal = "http://[fe80::1]";
