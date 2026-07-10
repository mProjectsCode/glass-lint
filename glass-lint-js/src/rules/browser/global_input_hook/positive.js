// @case description positive fixture for js:browser.global-input-hook
// @tool glass-lint rules=js:browser.global-input-hook
// @expect-error glass-lint rule=js:browser.global-input-hook message_id=detected
document.addEventListener("keydown", ()=>{});
// Migrated: interface/broad-input-hooks.js
// second independent example
// @expect-error glass-lint rule=js:browser.global-input-hook message_id=detected
window.addEventListener("keyup", () => {});
// @expect-error glass-lint rule=js:browser.global-input-hook message_id=detected
document.addEventListener("paste", () => {});

// Migrated: interface/dynamic-input-event-ignored.js
const legacyEventName = "keydown";
// @expect-error glass-lint rule=js:browser.global-input-hook message_id=detected
document.addEventListener(legacyEventName, () => {});
