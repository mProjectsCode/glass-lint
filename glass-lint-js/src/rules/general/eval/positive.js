// @case description positive fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval

function evaluate() {
    // @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
    eval("code");
}

// Global-object spellings and callable transforms share global eval identity.
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
globalThis.eval("global object");
const indirectEval = window.eval;
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
indirectEval("alias");
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
self.eval.call(null, "call");
const evalArgs = ["apply"];
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
global.eval.apply(null, evalArgs);
const boundEval = eval.bind(null, "bound");
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
boundEval();

// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
const run = new Function("return 1");

// Function is callable with or without `new`, including global-object access.
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
const calledFunction = window.Function("return 2");
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
const constructedFunction = new self.Function("return 3");

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
const runAsync = new AsyncFunction("return 1");
