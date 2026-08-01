// @case description positive fixture for js:dynamic-code.webassembly
// @tool glass-lint rules=js:dynamic-code.webassembly

// @expect-error glass-lint rule=js:dynamic-code.webassembly
WebAssembly.compile(bytes);
// @expect-error glass-lint rule=js:dynamic-code.webassembly
WebAssembly.compileStreaming(fetch(url));
// @expect-error glass-lint rule=js:dynamic-code.webassembly
WebAssembly.instantiate(bytes, imports);
// @expect-error glass-lint rule=js:dynamic-code.webassembly
WebAssembly.instantiateStreaming(fetch(url), imports);
// @expect-error glass-lint rule=js:dynamic-code.webassembly
WebAssembly.validate(bytes);

// Standard WebAssembly constructors retain the promoted global identity of
// their WebAssembly namespace members.
// @expect-error glass-lint rule=js:dynamic-code.webassembly
new WebAssembly.Module(bytes);
// @expect-error glass-lint rule=js:dynamic-code.webassembly
new WebAssembly.Instance(module);
// @expect-error glass-lint rule=js:dynamic-code.webassembly
new WebAssembly.Memory({ initial: 1 });
// @expect-error glass-lint rule=js:dynamic-code.webassembly
new WebAssembly.Table({ element: "anyfunc", initial: 1 });
// @expect-error glass-lint rule=js:dynamic-code.webassembly
new WebAssembly.Global({ value: "i32" }, 0);

// A stable alias of the configured namespace remains rooted.
const wasm = WebAssembly;
// @expect-error glass-lint rule=js:dynamic-code.webassembly
wasm.instantiate(bytes);
