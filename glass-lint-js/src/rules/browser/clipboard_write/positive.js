// @case description positive fixture for browser:browser.clipboard-write
// @tool glass-lint rules=browser:browser.clipboard-write
// @expect-error glass-lint rule=browser:browser.clipboard-write
navigator.clipboard.writeText("x");
// @expect-error glass-lint rule=browser:browser.clipboard-write
window.navigator.clipboard.write([]);
// @expect-error glass-lint rule=browser:browser.clipboard-write
self.navigator.clipboard.writeText("worker");
// @expect-error glass-lint rule=browser:browser.clipboard-write
globalThis.navigator.clipboard.writeText("global");
// Both write methods and derived aliases retain browser provenance.
// @expect-error glass-lint rule=browser:browser.clipboard-write
navigator.clipboard.write([]);
const writeClipboard = navigator.clipboard.writeText;
// @expect-error glass-lint rule=browser:browser.clipboard-write
writeClipboard("alias");
// @expect-error glass-lint rule=browser:browser.clipboard-write
document.execCommand("copy");
// @expect-error glass-lint rule=browser:browser.clipboard-write
window.document.execCommand("cut");
// @expect-error glass-lint rule=browser:browser.clipboard-write
globalThis.document.execCommand("copy");
// @expect-error glass-lint rule=browser:browser.clipboard-write
document.execCommand("cut");
