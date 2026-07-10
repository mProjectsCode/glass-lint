// @case description positive fixture for js:network.private-address
// @tool glass-lint rules=js:network.private-address
// @expect-error glass-lint rule=js:network.private-address message_id=detected
fetch("http://127.0.0.1");
// second independent example

// @expect-error glass-lint rule=js:network.private-address message_id=detected
fetch("http://10.0.0.2");

// @expect-error glass-lint rule=js:network.private-address message_id=detected
fetch("https://192.168.1.2");
