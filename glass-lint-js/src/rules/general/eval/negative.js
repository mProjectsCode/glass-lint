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

// Aliases and callable transforms of the local lookalike stay local.
const evalAlias = eval;
evalAlias("text");
eval.call(null, "text");

// Shadowed global-object spellings and mutated global properties fail closed.
function localWindow(window) {
  window.eval("local");
}
localWindow({ eval() {} });
globalThis.eval = safeParser;
// @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
globalThis.eval("mutated");

const globals = self;
globals.eval = safeParser;
// @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
self.eval("mutated through alias");

function localFunction(window) {
  // @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
  window.Function("return 'local'");
  // @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
  new window.Function("return 'local'");
}
localFunction({ Function() {} });
