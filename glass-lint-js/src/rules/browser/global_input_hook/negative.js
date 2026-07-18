// @case description negative fixture for browser:browser.global-input-hook
// @tool glass-lint rules=browser:browser.global-input-hook
// @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
// Other event types are outside the configured input set.
// @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.addEventListener("click", () => {});
// @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.body.addEventListener("load", () => {});
// @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.onclick = () => {};

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
    window.addEventListener("keydown", () => {});
}

function localDocument(document) {
    // @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
    document.onkeydown;
}
localWindow({ addEventListener() {} });

function localSelf(self) {
    // @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
    self.addEventListener("paste", () => {});
}
localSelf({ addEventListener() {} });

// Values that cannot be resolved statically are ignored.
function register(eventName) {
    // @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
    window.addEventListener(eventName, () => {});
}
