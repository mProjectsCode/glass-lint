// @case description Ported old classifier case: eval alias dynamic code
// @tool glass-lint rules=obsidian:dynamic_code

const run = eval; run("code"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
(0, eval)("code");
eval.call(null, "code");
const bound = eval.bind(globalThis); bound("code");
globalThis.eval("code");
window["eval"]("code");
