// @case description Ported old classifier case: eval alias dynamic code
// @tool glass-lint rules=obsidian:dynamic_code

const run = eval; run("code"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
(0, eval)("code"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
eval.call(null, "code"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
const bound = eval.bind(globalThis); bound("code"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
globalThis.eval("code"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
window["eval"]("code"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
