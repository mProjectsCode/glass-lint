// @case description negative fixture for js:browser.clipboard-write
// @tool glass-lint rules=js:browser.clipboard-write
// @expect-no-error glass-lint rule=js:browser.clipboard-write message_id=detected
// A locally defined navigator is not the browser global.
const navigator = { clipboard: { writeText() {} } };
// @expect-no-error glass-lint rule=js:browser.clipboard-write message_id=detected
navigator.clipboard.writeText("local");

// Reassignment drops a previously rooted alias.
let writeClipboard = globalThis.navigator.clipboard.writeText;
writeClipboard = () => {};
// @expect-no-error glass-lint rule=js:browser.clipboard-write message_id=detected
writeClipboard("local");
