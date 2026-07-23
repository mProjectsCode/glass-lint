// @case description negative fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval
// @expect-no-error glass-lint rule=js:dynamic-code.eval
// Shadowed global bindings are excluded.
const eval = () => {};
// @expect-no-error glass-lint rule=js:dynamic-code.eval
eval("local");

// Reassignment drops a global alias.
let run = globalThis.eval;
run = safeParser;
// @expect-no-error glass-lint rule=js:dynamic-code.eval
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
// @expect-no-error glass-lint rule=js:dynamic-code.eval
globalThis.eval("mutated");

const globals = self;
globals.eval = safeParser;
// @expect-no-error glass-lint rule=js:dynamic-code.eval
self.eval("mutated through alias");

function localFunction(window) {
  // @expect-no-error glass-lint rule=js:dynamic-code.eval
  window.Function("return 'local'");
  // @expect-no-error glass-lint rule=js:dynamic-code.eval
  new window.Function("return 'local'");
}
localFunction({ Function() {} });
