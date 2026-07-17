// @case description positive fixture for browser:browser.clipboard-write
// @tool glass-lint rules=browser:browser.clipboard-write
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
navigator.clipboard.writeText("x");
// Both write methods and derived aliases retain browser provenance.
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
navigator.clipboard.write([]);
const writeClipboard = navigator.clipboard.writeText;
// @expect-error glass-lint rule=browser:browser.clipboard-write message_id=detected
writeClipboard("alias");
