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
