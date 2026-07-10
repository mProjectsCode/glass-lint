// @case description negative fixture for js:browser.global-input-hook
// @tool glass-lint rules=js:browser.global-input-hook
// @expect-no-error glass-lint rule=js:browser.global-input-hook message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=js:browser.global-input-hook message_id=detected
document.addEventListener("click", () => {});
