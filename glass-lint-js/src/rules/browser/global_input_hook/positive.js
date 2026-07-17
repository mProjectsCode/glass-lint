// @case description positive fixture for browser:browser.global-input-hook
// @tool glass-lint rules=browser:browser.global-input-hook
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.addEventListener("keydown", ()=>{});
// The configured event set applies to both global receivers.
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
window.addEventListener("keyup", () => {});
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.addEventListener("paste", () => {});

// Resolved static constants are accepted as event names.
const eventName = "copy";
// @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
document.addEventListener(eventName, () => {});

// Deliberate heuristic gap: a shadowed local receiver is also reported.
function install(document) {
    // @expect-error glass-lint rule=browser:browser.global-input-hook message_id=detected
    document.addEventListener("cut", () => {});
}
