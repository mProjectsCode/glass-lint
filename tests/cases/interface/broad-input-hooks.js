// @case description Static keyboard event registration reports a broad input hook
// @tool glass-lint rules=js:browser.global-input-hook
// @tool eslint-obsidianmd config=recommended

document.addEventListener("keydown", () => {}); // @expect-error glass-lint rule=js:browser.global-input-hook message_id=detected
