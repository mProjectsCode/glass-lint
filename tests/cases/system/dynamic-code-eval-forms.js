// @case description Eval aliases are detected as dynamic code
// @tool glass-lint rules=js:dynamic-code.eval
// @tool eslint-obsidianmd config=recommended

const run = eval; run("code"); // @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
(0, eval)("code"); // @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
eval.call(null, "code"); // @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
const bound = eval.bind(globalThis); bound("code"); // @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
globalThis.eval("code"); // @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
window["eval"]("code"); // @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
