// @case description negative fixture for obsidian:network.request
// @tool glass-lint rules=obsidian:network.request
// @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
function request() {}
request("https://example.com");
