// @case description positive fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval

function evaluate() {
    // @expect-error glass-lint rule=js:dynamic-code.eval
    eval("code");
}

// Global-object spellings and callable transforms share global eval identity.
// @expect-error glass-lint rule=js:dynamic-code.eval
globalThis.eval("global object");
const indirectEval = window.eval;
// @expect-error glass-lint rule=js:dynamic-code.eval
indirectEval("alias");
// @expect-error glass-lint rule=js:dynamic-code.eval
self.eval.call(null, "call");
const evalArgs = ["apply"];
// @expect-error glass-lint rule=js:dynamic-code.eval
global.eval.apply(null, evalArgs);
const boundEval = eval.bind(null, "bound");
// @expect-error glass-lint rule=js:dynamic-code.eval
boundEval();

// @expect-error glass-lint rule=js:dynamic-code.eval
const run = new Function("return 1");

// Function is callable with or without `new`, including global-object access.
// @expect-error glass-lint rule=js:dynamic-code.eval
const calledFunction = window.Function("return 2");
// @expect-error glass-lint rule=js:dynamic-code.eval
const constructedFunction = new self.Function("return 3");

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
// @expect-error glass-lint rule=js:dynamic-code.eval
const runAsync = new AsyncFunction("return 1");
