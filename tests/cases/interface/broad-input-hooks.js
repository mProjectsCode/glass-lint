// @case description Static keyboard event registration reports a broad input hook
// @tool glass-lint rules=obsidian:browser.broad_input_hooks

document.addEventListener("keydown", () => {}); // @expect-error glass-lint rule=obsidian:browser.broad_input_hooks message_id=detected
