// @case description positive fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
eval("code");
// Migrated: system/dynamic-code-eval-forms.js
const evalAlias = eval;
evalAlias("code");
(0, eval)("code");
eval.call(null, "code");
const boundEval = eval.bind(globalThis);
boundEval("code");
globalThis.eval("code");
// Migrated: system/dynamic-code-function-constructors.js
Function("return 1")();
const function = Function;
function("return 1")();
