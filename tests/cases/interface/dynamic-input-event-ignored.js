// @case description Constant keyboard event aliases count, while unrelated event names do not
// @tool glass-lint rules=obsidian:browser.broad_input_hooks
// @tool eslint-obsidianmd config=recommended

document.addEventListener("keydown", () => {}); // @expect-error glass-lint rule=obsidian:browser.broad_input_hooks message_id=detected

const eventName = "keydown";
document.addEventListener(eventName, () => {}); // @expect-error glass-lint rule=obsidian:browser.broad_input_hooks message_id=detected

const key = "keydown";
document.addEventListener("click", () => {});
