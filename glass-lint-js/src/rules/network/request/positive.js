// @case description positive fixture for js:network.request
// @tool glass-lint rules=js:network.request
// @expect-error glass-lint rule=js:network.request message_id=detected
fetch("https://example.com");
// second independent example
// @expect-error glass-lint rule=js:network.request message_id=detected
fetch("/second");
