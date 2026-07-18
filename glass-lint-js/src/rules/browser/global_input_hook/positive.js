// @case description positive fixture for browser:browser.global-input-hook
// @tool glass-lint rules=browser:browser.global-input-hook
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.addEventListener("keydown", ()=>{});
// The configured event set applies to both global receivers.
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
window.addEventListener("keyup", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.addEventListener("paste", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.addEventListener("pointerdown", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
window.addEventListener("touchstart", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
globalThis.addEventListener("input", () => {});
// The worker/global-object spelling retains the same rooted provenance.
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
self.addEventListener("paste", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.body.addEventListener("drop", () => {});

// Rooted property writes are intentionally not reported; see the rule docs.
// @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.onkeydown = () => {};
// @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
window.onpaste = () => {};

// Resolved static constants are accepted as event names.
const eventName = "copy";
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.addEventListener(eventName, () => {});

// Rooted listener calls reject shadowed local receivers.
function install(document) {
    // @expect-no-error glass-lint rule=browser:browser.global-input-hook message_id=detected
    document.addEventListener("cut", () => {});
}
