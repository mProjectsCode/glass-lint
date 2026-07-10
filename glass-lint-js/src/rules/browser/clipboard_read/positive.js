// @case description positive fixture for js:browser.clipboard-read
// @tool glass-lint rules=js:browser.clipboard-read
// @expect-error glass-lint rule=js:browser.clipboard-read message_id=detected
navigator.clipboard.readText();
// second independent example

// @expect-error glass-lint rule=js:browser.clipboard-read message_id=detected
navigator.clipboard.read();
const readClipboard = navigator.clipboard.readText;

// @expect-error glass-lint rule=js:browser.clipboard-read message_id=detected
readClipboard();
