// @case description negative fixture for js:browser.global-input-hook
// @tool glass-lint rules=js:browser.global-input-hook
// @expect-no-error glass-lint rule=js:browser.global-input-hook message_id=detected
// Other event types are outside the configured input set.
// @expect-no-error glass-lint rule=js:browser.global-input-hook message_id=detected
document.addEventListener("click", () => {});

// Values that cannot be resolved statically are ignored.
function register(eventName) {
    // @expect-no-error glass-lint rule=js:browser.global-input-hook message_id=detected
    window.addEventListener(eventName, () => {});
}
