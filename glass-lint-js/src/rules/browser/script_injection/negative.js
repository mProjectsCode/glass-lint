// @case description negative fixture for browser:dynamic-code.script-injection
// @tool glass-lint rules=browser:dynamic-code.script-injection
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
// Other static tags and dynamic tag names do not match.
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
document.createElement("div");
document.createElement(tagName);
// Non-executable markup and dynamic payloads are not proven script injection.
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
document.write("<div>safe</div>");
const markup = getMarkup();
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
document.writeln(markup);
// Constant concatenation is folded, but creation alone is not executable flow.
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
document.createElement("scr" + "ipt");

// Aliasing createElement is followed and matches.
const create = document.createElement;
// @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
create("script");

function localDocument(document) {
    // @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
    document.createElement("script");
    // @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
    document.write("<script>local</script>");
}
localDocument({ createElement() {}, write() {} });

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
    window.document.createElement("script");
    // @expect-no-error glass-lint rule=browser:dynamic-code.script-injection
    window.document.writeln("<script>local</script>");
}
localWindow({ document: { createElement() {}, writeln() {} } });
