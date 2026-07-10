// @case description Each fetch call produces a located finding
// @tool glass-lint rules=js:network.request
// @tool eslint-obsidianmd config=recommended

fetch('/one'); // @expect-error glass-lint rule=js:network.request message_id=detected
fetch('/two'); // @expect-error glass-lint rule=js:network.request message_id=detected
