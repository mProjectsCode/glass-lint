// @case description positive fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
eval("code");

// Migrated: system/dynamic-code-eval-forms.js
const legacyEvalAlias = eval;
legacyEvalAlias("code");
(0, eval)("code");
eval.call(null, "code");
const legacyBoundEval = eval.bind(globalThis);
legacyBoundEval("code");
globalThis.eval("code");

// Migrated: system/dynamic-code-function-constructors.js
Function("return 1")();
const LegacyFunction = Function;
LegacyFunction("return 1")();
