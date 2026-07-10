// @case description negative fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval
// @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
// Shadowed global bindings are excluded.
const eval = () => {};
// @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
eval("local");

// Reassignment drops a global alias.
let run = globalThis.eval;
run = safeParser;
// @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
run("text");

// Documented gaps: bare-eval aliases and eval.call are not currently detected.
const evalAlias = eval;
evalAlias("text");
eval.call(null, "text");
