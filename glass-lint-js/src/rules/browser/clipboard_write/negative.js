// @case description negative fixture for browser:browser.clipboard-write
// @tool glass-lint rules=browser:browser.clipboard-write
// @expect-no-error glass-lint rule=browser:browser.clipboard-write message_id=detected
// A locally defined navigator is not the browser global.
const navigator = { clipboard: { writeText() {} } };
// @expect-no-error glass-lint rule=browser:browser.clipboard-write message_id=detected
navigator.clipboard.writeText("local");

// Reassignment drops a previously rooted alias.
let writeClipboard = globalThis.navigator.clipboard.writeText;
writeClipboard = () => {};
// @expect-no-error glass-lint rule=browser:browser.clipboard-write message_id=detected
writeClipboard("local");

function localDocument(document) {
    // @expect-no-error glass-lint rule=browser:browser.clipboard-write message_id=detected
    document.execCommand("copy");
}
localDocument({ execCommand() {} });

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.clipboard-write message_id=detected
    window.document.execCommand("copy");
}
localWindow({ document: { execCommand() {} } });

function localWindowNavigator(window) {
    // @expect-no-error glass-lint rule=browser:browser.clipboard-write message_id=detected
    window.navigator.clipboard.writeText("local");
}
localWindowNavigator({ navigator: { clipboard: { writeText() {} } } });
