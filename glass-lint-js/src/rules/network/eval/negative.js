// @case description negative fixture for js:dynamic-code.eval
// @tool glass-lint rules=js:dynamic-code.eval
// @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
function localLookalike() { return null; }
localLookalike();
const eval = () => {};
// @expect-no-error glass-lint rule=js:dynamic-code.eval message_id=detected
eval("local");

// Migrated: system/dynamic-code-negative-flow.js
function legacyShadowedDynamicCode(eval, Function, setTimeout) {
  eval("text");
  Function("text");
  setTimeout("text", 0);
}
let legacyRun = globalThis.eval;
legacyRun = safeParser;
legacyRun("text");
setTimeout(() => runCode(), 0);
