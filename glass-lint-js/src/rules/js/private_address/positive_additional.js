// @case description additional marker coverage for js:network.private-address
// @tool glass-lint rules=js:network.private-address

// These markers are tested separately because the shared evidence limit keeps
// a single case from asserting every configured literal marker at once.
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const localhost = "localhost";
// @expect-error glass-lint rule=js:network.private-address message_id=detected
const httpsLan = "https://192.168.1.2";
