// @case description negative fixture for js:concurrency.shared-memory
// @tool glass-lint rules=js:concurrency.shared-memory

function localAtomics(Atomics, SharedArrayBuffer) {
    // @expect-no-error glass-lint rule=js:concurrency.shared-memory
    Atomics.wait(view, 0, 0);
    // @expect-no-error glass-lint rule=js:concurrency.shared-memory
    new SharedArrayBuffer(8);
}
localAtomics({ wait() {} }, class {});

const method = getMethod();
// @expect-no-error glass-lint rule=js:concurrency.shared-memory
Atomics[method](view, 0);
