// @case description negative fixture for js:browser.clipboard-read
// @tool glass-lint rules=js:browser.clipboard-read
// @expect-no-error glass-lint rule=js:browser.clipboard-read message_id=detected
// A locally defined navigator is not the browser global.
const navigator = { clipboard: { readText() {} } };
// @expect-no-error glass-lint rule=js:browser.clipboard-read message_id=detected
navigator.clipboard.readText();

// Reassignment drops a previously rooted alias.
let readClipboard = globalThis.navigator.clipboard.readText;
readClipboard = () => {};
// @expect-no-error glass-lint rule=js:browser.clipboard-read message_id=detected
readClipboard();
