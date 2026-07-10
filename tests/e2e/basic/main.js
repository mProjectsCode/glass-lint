// @case description End-to-end linting reports a browser capability
// @tool glass-lint rules=js:network.request

fetch("https://example.com"); // @expect-error glass-lint rule=js:network.request message_id=detected
