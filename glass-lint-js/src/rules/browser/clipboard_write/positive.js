// @case description positive fixture for browser:browser.clipboard-write
// @tool glass-lint rules=browser:browser.clipboard-write
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
navigator.clipboard.writeText("x");
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
window.navigator.clipboard.write([]);
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
self.navigator.clipboard.writeText("worker");
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
globalThis.navigator.clipboard.writeText("global");
// Both write methods and derived aliases retain browser provenance.
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
navigator.clipboard.write([]);
const writeClipboard = navigator.clipboard.writeText;
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
writeClipboard("alias");
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
document.execCommand("copy");
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
window.document.execCommand("cut");
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
globalThis.document.execCommand("copy");
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
document.execCommand("cut");
