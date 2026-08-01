// @case description positive fixture for js:concurrency.shared-memory
// @tool glass-lint rules=js:concurrency.shared-memory

// @expect-error glass-lint rule=js:concurrency.shared-memory
const buffer = new SharedArrayBuffer(1024);
// @expect-error glass-lint rule=js:concurrency.shared-memory
Atomics.store(view, 0, 1);
// @expect-error glass-lint rule=js:concurrency.shared-memory
Atomics.load(view, 0);
// @expect-error glass-lint rule=js:concurrency.shared-memory
Atomics.wait(view, 0, 0);
// @expect-error glass-lint rule=js:concurrency.shared-memory
Atomics.notify(view, 0);
// @expect-error glass-lint rule=js:concurrency.shared-memory
Atomics.compareExchange(view, 0, 0, 1);
