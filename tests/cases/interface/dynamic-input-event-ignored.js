// @case description Constant keyboard event aliases count, while unrelated event names do not
// @tool glass-lint rules=js:browser.global-input-hook
// @tool eslint-obsidianmd config=recommended

document.addEventListener("keydown", () => {}); // @expect-error glass-lint rule=js:browser.global-input-hook message_id=detected

const eventName = "keydown";
document.addEventListener(eventName, () => {}); // @expect-error glass-lint rule=js:browser.global-input-hook message_id=detected

const key = "keydown";
document.addEventListener("click", () => {});
