// @case description positive fixture for browser:browser.global-input-hook
// @tool glass-lint rules=browser:browser.global-input-hook
// @expect-error glass-lint rule=browser:browser.global-input-hook
document.addEventListener("keydown", ()=>{});
// The configured event set applies to both global receivers.
// @expect-error glass-lint rule=browser:browser.global-input-hook
window.addEventListener("keyup", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook
document.addEventListener("paste", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook
document.addEventListener("pointerdown", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook
window.addEventListener("touchstart", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook
globalThis.addEventListener("input", () => {});
// The worker/global-object spelling retains the same rooted provenance.
// @expect-error glass-lint rule=browser:browser.global-input-hook
self.addEventListener("paste", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook
document.body.addEventListener("drop", () => {});

// Rooted property writes are intentionally not reported; see the rule docs.
// @expect-no-error glass-lint rule=browser:browser.global-input-hook
document.onkeydown = () => {};
// @expect-no-error glass-lint rule=browser:browser.global-input-hook
window.onpaste = () => {};

// Resolved static constants are accepted as event names.
const eventName = "copy";
// @expect-error glass-lint rule=browser:browser.global-input-hook
document.addEventListener(eventName, () => {});

// Rooted listener calls reject shadowed local receivers.
function install(document) {
    // @expect-no-error glass-lint rule=browser:browser.global-input-hook
    document.addEventListener("cut", () => {});
}
