// @case description negative fixture for browser:browser.clipboard-read
// @tool glass-lint rules=browser:browser.clipboard-read
// @expect-no-error glass-lint rule=browser:browser.clipboard-read
// A locally defined navigator is not the browser global.
const navigator = { clipboard: { readText() {} } };
// @expect-no-error glass-lint rule=browser:browser.clipboard-read
navigator.clipboard.readText();

// Reassignment drops a previously rooted alias.
let readClipboard = globalThis.navigator.clipboard.readText;
readClipboard = () => {};
// @expect-no-error glass-lint rule=browser:browser.clipboard-read
readClipboard();

function localDocument(document) {
    // @expect-no-error glass-lint rule=browser:browser.clipboard-read
    document.execCommand("paste");
}
localDocument({ execCommand() {} });

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.clipboard-read
    window.document.execCommand("paste");
}
localWindow({ document: { execCommand() {} } });

function localWindowNavigator(window) {
    // @expect-no-error glass-lint rule=browser:browser.clipboard-read
    window.navigator.clipboard.readText();
}
localWindowNavigator({ navigator: { clipboard: { readText() {} } } });
