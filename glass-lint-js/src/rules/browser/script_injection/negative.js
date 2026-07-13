// @case description negative fixture for js:dynamic-code.script-injection
// @tool glass-lint rules=js:dynamic-code.script-injection
// @expect-no-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
// Other static tags and dynamic tag names do not match.
// @expect-no-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("div");
document.createElement(tagName);
// Constant concatenation is folded and therefore matches.
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
document.createElement("scr" + "ipt");

// Aliasing createElement is followed and matches.
const create = document.createElement;
// @expect-error glass-lint rule=js:dynamic-code.script-injection message_id=detected
create("script");
