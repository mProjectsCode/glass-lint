// @case description positive fixture for browser:browser.clipboard-read
// @tool glass-lint rules=browser:browser.clipboard-read
// @expect-error glass-lint rule=browser:browser.clipboard-read message_id=detected
navigator.clipboard.readText();
// @expect-error glass-lint rule=browser:browser.clipboard-read message_id=detected
window.navigator.clipboard.read();
// @expect-error glass-lint rule=browser:browser.clipboard-read message_id=detected
self.navigator.clipboard.readText();
// @expect-error glass-lint rule=browser:browser.clipboard-read message_id=detected
globalThis.navigator.clipboard.readText();
// Both read methods and derived aliases retain browser provenance.
// @expect-error glass-lint rule=browser:browser.clipboard-read message_id=detected
navigator.clipboard.read();
const readClipboard = navigator.clipboard.readText;
// @expect-error glass-lint rule=browser:browser.clipboard-read message_id=detected
readClipboard();
// @expect-error glass-lint rule=browser:browser.clipboard-read message_id=detected
document.execCommand("paste");
// @expect-error glass-lint rule=browser:browser.clipboard-read message_id=detected
window.document.execCommand("paste");
// @expect-error glass-lint rule=browser:browser.clipboard-read message_id=detected
globalThis.document.execCommand("paste");
