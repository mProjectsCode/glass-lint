// @case description negative fixture for js:dynamic-code.webassembly
// @tool glass-lint rules=js:dynamic-code.webassembly

// Local namespace-shaped objects do not establish WebAssembly provenance.
function localApi(WebAssembly) {
    // @expect-no-error glass-lint rule=js:dynamic-code.webassembly
    WebAssembly.compile(bytes);
    // @expect-no-error glass-lint rule=js:dynamic-code.webassembly
    new WebAssembly.Module(bytes);
}
localApi({ compile() {} , Module: class {} });

// A reassigned rooted member is no longer trusted.
WebAssembly.compile = safeCompile;
// @expect-no-error glass-lint rule=js:dynamic-code.webassembly
WebAssembly.compile(bytes);

// Dynamic property selection is outside the static rooted matcher.
const method = getMethod();
// @expect-no-error glass-lint rule=js:dynamic-code.webassembly
WebAssembly[method](bytes);

// Local constructor lookalikes are excluded.
class Module {}
// @expect-no-error glass-lint rule=js:dynamic-code.webassembly
new Module(bytes);
