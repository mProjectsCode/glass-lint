// @case description positive fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval

function evaluate() {
    // @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
    eval("code");
}

// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
const run = new Function("return 1");

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
// @expect-error glass-lint rule=js:dynamic-code.eval message_id=detected
const runAsync = new AsyncFunction("return 1");
