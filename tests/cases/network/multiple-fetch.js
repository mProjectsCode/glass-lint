// @case description Each fetch call produces a located finding
// @tool glass-lint rules=obsidian:network.browser

fetch('/one'); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
fetch('/two'); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
