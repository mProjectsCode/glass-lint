// @case description negative fixture for browser:dynamic-code.script-injection
// @tool glass-lint rules=browser:dynamic-code.script-injection
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
// Other static tags and dynamic tag names do not match.
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
document.createElement("div");
document.createElement(tagName);
// Non-executable markup and dynamic payloads are not proven script injection.
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
document.write("<div>safe</div>");
const markup = getMarkup();
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
document.writeln(markup);
// Constant concatenation is folded and therefore matches.
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
document.createElement("scr" + "ipt");

// Aliasing createElement is followed and matches.
const create = document.createElement;
// @expect-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
create("script");

function localDocument(document) {
    // @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
    document.createElement("script");
    // @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
    document.write("<script>local</script>");
}
localDocument({ createElement() {}, write() {} });

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
    window.document.createElement("script");
    // @expect-no-error glass-lint rule=browser:dynamic-code.script-injection message_id=detected
    window.document.writeln("<script>local</script>");
}
localWindow({ document: { createElement() {}, writeln() {} } });
