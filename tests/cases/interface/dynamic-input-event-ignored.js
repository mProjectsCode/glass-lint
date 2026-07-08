// @case description Ported old classifier cases: dynamic and unrelated input event names do not count
// @tool glass-lint rules=obsidian:browser.broad_input_hooks

const eventName = "keydown";
document.addEventListener(eventName, () => {});

const key = "keydown";
document.addEventListener("click", () => {});
