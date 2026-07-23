// @case description positive fixture for browser:browser.clipboard-read
// @tool glass-lint rules=browser:browser.clipboard-read
// @expect-error glass-lint rule=browser:browser.clipboard-read
navigator.clipboard.readText();
// @expect-error glass-lint rule=browser:browser.clipboard-read
window.navigator.clipboard.read();
// @expect-error glass-lint rule=browser:browser.clipboard-read
self.navigator.clipboard.readText();
// @expect-error glass-lint rule=browser:browser.clipboard-read
globalThis.navigator.clipboard.readText();
// Both read methods and derived aliases retain browser provenance.
// @expect-error glass-lint rule=browser:browser.clipboard-read
navigator.clipboard.read();
const readClipboard = navigator.clipboard.readText;
// @expect-error glass-lint rule=browser:browser.clipboard-read
readClipboard();
// @expect-error glass-lint rule=browser:browser.clipboard-read
document.execCommand("paste");
// @expect-error glass-lint rule=browser:browser.clipboard-read
window.document.execCommand("paste");
// @expect-error glass-lint rule=browser:browser.clipboard-read
globalThis.document.execCommand("paste");
