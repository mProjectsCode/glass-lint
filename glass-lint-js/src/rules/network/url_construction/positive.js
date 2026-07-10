// @case description positive fixture for js:network.url-construction
// @tool glass-lint rules=js:network.url-construction
// @expect-error glass-lint rule=js:network.url-construction message_id=detected
new URL("https://example.com");
// second independent example
// @expect-error glass-lint rule=js:network.url-construction message_id=detected
new URLSearchParams("a=1");
// @expect-error glass-lint rule=js:network.url-construction message_id=detected
new URL("/relative");
